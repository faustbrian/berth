//! config api load save module.
//!
//! Contains config api load save logic used by Berth command workflows.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

use super::super::{Config, ContainerEngine, ProjectType, RawConfig, expansion, validation};
use super::project::ProjectRootPathOptions;

pub type RawConfigPathOptions<'a> = ProjectRootPathOptions<'a>;
pub type LoadConfigPathOptions<'a> = ProjectRootPathOptions<'a>;
pub type SaveConfigPathOptions<'a> = ProjectRootPathOptions<'a>;

/// Loads the configuration file using default discovery.
///
/// # Errors
///
/// Returns an error if the config cannot be found, read, or parsed.
pub fn load_config() -> Result<Config> {
    load_config_with(LoadConfigPathOptions::new(None, None))
}

/// Loads configuration from explicit config path or project root override.
///
/// # Errors
///
/// Returns an error if path resolution, reading, or parsing fails.
pub fn load_config_with(options: LoadConfigPathOptions<'_>) -> Result<Config> {
    let raw_config = load_raw_config_with(options)?;
    let mut config = expansion::expand_raw_config(raw_config)?;
    let project_root = super::project::project_root_with(ProjectRootPathOptions::new(
        options.config_path,
        options.project_root,
    ))?;

    validation::validate_and_resolve_domains(&mut config, &project_root)?;
    validation::validate_and_resolve_container_names(&mut config)?;
    validation::validate_swarm_targets(&config)?;

    Ok(config)
}

/// Loads raw config with from persisted or external state.
pub(crate) fn load_raw_config_with(options: RawConfigPathOptions<'_>) -> Result<RawConfig> {
    let config_path = super::config_io::resolve_config_path(options)?;
    let mut raw: RawConfig =
        super::config_io::read_config_file(&config_path, "config file", "config file")?;
    raw.project_type = Some(resolve_project_type(&raw, &config_path)?);
    Ok(raw)
}

/// Loads the configured container runtime engine from raw config.
///
/// # Errors
///
/// Returns an error if path resolution, reading, or parsing fails.
pub fn load_container_engine_with(
    options: LoadConfigPathOptions<'_>,
) -> Result<Option<ContainerEngine>> {
    let raw = load_raw_config_with(options)?;
    Ok(raw.container_engine)
}

fn resolve_project_type(raw: &RawConfig, config_path: &Path) -> Result<ProjectType> {
    if let Some(project_type) = raw.project_type {
        return Ok(project_type);
    }

    let composer_path = config_path
        .parent()
        .map(|parent| parent.join("composer.json"))
        .ok_or_else(|| {
            anyhow::anyhow!(
                "config path '{}' has no parent directory",
                config_path.display()
            )
        })?;
    if !composer_path.exists() {
        anyhow::bail!(
            "Unable to resolve project_type: set .berth.toml or .berth.yaml project_type or composer.json type \
             (\"project\" or \"library\"). composer.json not found at {}",
            composer_path.display()
        );
    }

    let composer_content = std::fs::read_to_string(&composer_path).with_context(|| {
        format!(
            "failed to read composer.json at {}",
            composer_path.display()
        )
    })?;
    let composer_json: serde_json::Value =
        serde_json::from_str(&composer_content).with_context(|| {
            format!(
                "failed to parse composer.json at {}",
                composer_path.display()
            )
        })?;
    let composer_type = composer_json
        .get("type")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "Unable to resolve project_type: set .berth.toml or .berth.yaml project_type or composer.json \
                 type (\"project\" or \"library\"). composer.json missing \"type\" at {}",
                composer_path.display()
            )
        })?;

    match composer_type {
        "project" => Ok(ProjectType::Project),
        "library" => Ok(ProjectType::Library),
        _ => anyhow::bail!(
            "Unable to resolve project_type: set .berth.toml or .berth.yaml project_type or composer.json type \
             (\"project\" or \"library\"). composer.json type was \"{}\" at {}",
            composer_type,
            composer_path.display()
        ),
    }
}

/// Saves configuration back to the detected Berth config format.
///
/// # Errors
///
/// Returns an error if the config path cannot be resolved or writing fails.
pub fn save_config_with(config: &Config, options: SaveConfigPathOptions<'_>) -> Result<PathBuf> {
    let path = super::config_io::resolve_config_path(options)?;
    super::config_io::write_config_file(&path, config, "config", "config file")?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::{LoadConfigPathOptions, load_container_engine_with};
    use crate::config::ContainerEngine;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_root() -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("berth-load-save-{nanos}"));
        drop(fs::remove_dir_all(&root));
        fs::create_dir_all(&root).expect("create temp root");
        root
    }

    #[test]
    fn load_container_engine_prefers_configured_value() {
        let root = temp_root();
        let config_path = root.join(".berth.toml");
        fs::write(
            &config_path,
            "schema_version = 1\nproject_type = \"project\"\ncontainer_engine = \"podman\"\nservice = []\n",
        )
        .expect("write config");

        let engine =
            load_container_engine_with(LoadConfigPathOptions::new(Some(&config_path), None))
                .expect("load engine");

        assert_eq!(engine, Some(ContainerEngine::Podman));
    }

    #[test]
    fn load_container_engine_detects_project_type_from_composer_type() {
        let root = temp_root();
        let config_path = root.join(".berth.toml");
        fs::write(
            &config_path,
            "schema_version = 1\ncontainer_engine = \"podman\"\nservice = []\n",
        )
        .expect("write config");
        fs::write(root.join("composer.json"), r#"{"type":"library"}"#).expect("write composer");

        let engine =
            load_container_engine_with(LoadConfigPathOptions::new(Some(&config_path), None))
                .expect("load engine");

        assert_eq!(engine, Some(ContainerEngine::Podman));
    }

    #[test]
    fn load_container_engine_fails_when_project_type_is_unresolved() {
        let root = temp_root();
        let config_path = root.join(".berth.toml");
        fs::write(&config_path, "schema_version = 1\nservice = []\n").expect("write config");

        let error =
            load_container_engine_with(LoadConfigPathOptions::new(Some(&config_path), None))
                .expect_err("project type should be required");
        assert!(error.to_string().contains("Unable to resolve project_type"));
    }

    #[test]
    fn load_container_engine_supports_yaml_config() {
        let root = temp_root();
        let config_path = root.join(".berth.yaml");
        fs::write(
            &config_path,
            "schema_version: 1\nproject_type: project\ncontainer_engine: podman\nservice: []\nswarm: []\n",
        )
        .expect("write config");

        let engine =
            load_container_engine_with(LoadConfigPathOptions::new(Some(&config_path), None))
                .expect("load engine");

        assert_eq!(engine, Some(ContainerEngine::Podman));
    }

    #[test]
    fn save_config_preserves_yaml_format() {
        let root = temp_root();
        let config_path = root.join(".berth.yaml");
        fs::write(
            &config_path,
            "schema_version: 1\nproject_type: project\ncontainer_prefix: demo\nservice: []\nswarm: []\n",
        )
        .expect("write config");

        let mut config =
            super::load_config_with(LoadConfigPathOptions::new(Some(&config_path), None))
                .expect("load config");
        config.container_prefix = Some("updated".to_owned());

        let saved_path = super::save_config_with(
            &config,
            super::SaveConfigPathOptions::new(Some(&config_path), None),
        )
        .expect("save config");
        let saved_content = fs::read_to_string(saved_path).expect("read saved config");

        assert!(saved_content.contains("container_prefix: updated"));
        assert!(!saved_content.contains("container_prefix = \"updated\""));
    }
}
