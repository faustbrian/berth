//! config api load save module.
//!
//! Contains config api load save logic used by Berth command workflows.

use anyhow::{Context, Result};
use std::path::Path;

use super::super::{Config, ContainerEngine, ProjectType, RawConfig, expansion, validation};
use super::project::ProjectRootPathOptions;

pub type RawConfigPathOptions<'a> = ProjectRootPathOptions<'a>;
pub type LoadConfigPathOptions<'a> = ProjectRootPathOptions<'a>;

/// Loads configuration from explicit config path or project root override.
///
/// # Errors
///
/// Returns an error if path resolution, reading, or parsing fails.
pub fn load_config_with(options: LoadConfigPathOptions<'_>) -> Result<Config> {
    let config_path = super::config_io::resolve_config_path(options)?;
    let raw_config = load_raw_config_with(options)?;
    // Errors past the YAML parser name a service or field but not the file,
    // which matters once runtime-environment and swarm-source files exist.
    let in_file = || format!("invalid project file {}", config_path.display());
    let mut config = expansion::expand_raw_config(raw_config).with_context(in_file)?;
    let project_root = super::project::project_root_with(ProjectRootPathOptions::new(
        options.config_path,
        options.project_root,
    ))?;

    validation::validate_and_resolve_container_names(&mut config).with_context(in_file)?;
    validation::validate_swarm_targets(&config).with_context(in_file)?;
    validation::validate_service_dependencies(&config).with_context(in_file)?;
    let identity = crate::instance::resolve_instance(&project_root, options.instance)?;
    // Not `in_file`: naming the instance reads berth-home state (the
    // installation ID, tenant passwords), whose errors are not the file's.
    super::super::instance_naming::apply_instance_identity(&mut config, &identity)?;

    Ok(config)
}

/// Loads raw config with from persisted or external state.
pub(crate) fn load_raw_config_with(options: RawConfigPathOptions<'_>) -> Result<RawConfig> {
    let config_path = super::config_io::resolve_config_path(options)?;
    let mut raw = super::config_io::read_project_file(&config_path)?;
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
    // Only an app berth runs (Laravel) needs to know which it is; a project
    // file of databases, caches and other services alone is a project.
    let has_app = raw.service.iter().any(|service| {
        service.kind == Some(crate::config::Kind::App)
            || service.preset.as_deref().is_some_and(|preset| {
                // An unknown preset fails when the services are expanded.
                super::super::presets::preset_defaults(preset)
                    .is_ok_and(|defaults| defaults.kind == crate::config::Kind::App)
            })
    });
    // Whatever composer.json says, if it says anything.
    if !has_app {
        return Ok(ProjectType::Project);
    }
    if !composer_path.exists() {
        anyhow::bail!(
            "{} has no `project_type` and there is no {} to read it from; add `project_type: \
             project` (an app berth runs) or `project_type: library` (a package) to {}",
            config_path.display(),
            composer_path.display(),
            config_path.display()
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
        .and_then(serde_json::Value::as_str);
    let Some(composer_type) = composer_type else {
        anyhow::bail!(
            "{} has no `project_type` and {} has no \"type\"; add `project_type: project` \
             (an app berth runs) or `project_type: library` (a package) to {}",
            config_path.display(),
            composer_path.display(),
            config_path.display()
        );
    };

    match composer_type {
        "project" => Ok(ProjectType::Project),
        "library" => Ok(ProjectType::Library),
        _ => anyhow::bail!(
            "{} has no `project_type`, and {} says \"type\": \"{composer_type}\", which is \
             neither \"project\" nor \"library\"; add `project_type: project` or \
             `project_type: library` to {}",
            config_path.display(),
            composer_path.display(),
            config_path.display()
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::{LoadConfigPathOptions, load_container_engine_with};
    use crate::config::ContainerEngine;
    use std::fs;
    use std::path::PathBuf;

    fn temp_root() -> PathBuf {
        let nanos = crate::test_support::unique_stamp();
        let root = crate::test_support::temp_root().join(format!("berth-load-save-{nanos}"));
        drop(fs::remove_dir_all(&root));
        fs::create_dir_all(&root).expect("create temp root");
        root
    }

    #[test]
    fn load_container_engine_prefers_configured_value() {
        let root = temp_root();
        let config_path = root.join(".berth.yaml");
        crate::config::project_file::write_test_config_file(&config_path,
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
        let config_path = root.join(".berth.yaml");
        crate::config::project_file::write_test_config_file(
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
        let config_path = root.join(".berth.yaml");
        crate::config::project_file::write_test_config_file(
            &config_path,
            "schema_version = 1\n\n[[service]]\npreset = \"laravel\"\n",
        )
        .expect("write config");

        let error =
            load_container_engine_with(LoadConfigPathOptions::new(Some(&config_path), None))
                .expect_err("an app's project type should be required");
        assert!(error.to_string().contains("has no `project_type`"));
    }

    #[test]
    fn a_project_file_without_an_app_ignores_a_type_less_composer_json() {
        let root = temp_root();
        let config_path = root.join(".berth.yaml");
        fs::write(
            &config_path,
            "schema_version: 9\nproject: tests\nservices:\n  db:\n    preset: postgres\n    isolation: dedicated\n",
        )
        .expect("write config");
        fs::write(root.join("composer.json"), "{}").expect("write composer");
        let config =
            crate::config::load_config_with(LoadConfigPathOptions::new(Some(&config_path), None))
                .expect("loads without project_type");
        assert_eq!(config.project_type, crate::config::ProjectType::Project);
    }

    #[test]
    fn a_project_file_without_an_app_needs_no_project_type() {
        // Only an app berth runs needs to know which kind it is.
        let root = temp_root();
        let config_path = root.join(".berth.yaml");
        fs::write(
            &config_path,
            "schema_version: 9\nproject: tests\nservices:\n  db:\n    preset: postgres\n    isolation: dedicated\n",
        )
        .expect("write config");
        let config =
            crate::config::load_config_with(LoadConfigPathOptions::new(Some(&config_path), None))
                .expect("loads without project_type");
        assert_eq!(config.project_type, crate::config::ProjectType::Project);
    }

    #[test]
    fn load_container_engine_supports_yaml_config() {
        let root = temp_root();
        let config_path = root.join(".berth.yaml");
        fs::write(
            &config_path,
            "schema_version: 9\nproject: demo\nproject_type: project\ncontainer_engine: podman\n",
        )
        .expect("write config");

        let engine =
            load_container_engine_with(LoadConfigPathOptions::new(Some(&config_path), None))
                .expect("load engine");

        assert_eq!(engine, Some(ContainerEngine::Podman));
    }
}
