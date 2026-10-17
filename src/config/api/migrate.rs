//! config api migrate module.
//!
//! Contains config api migrate logic used by Berth command workflows.

use anyhow::Result;
use std::path::PathBuf;

use super::super::{expansion, validation};
use super::load_save::{
    RawConfigPathOptions, SaveConfigPathOptions, load_raw_config_with, save_config_with,
};
use super::project::ProjectRootPathOptions;

pub type MigrateConfigOptions<'a> = ProjectRootPathOptions<'a>;

pub fn migrate_config_with(options: MigrateConfigOptions<'_>) -> Result<PathBuf> {
    let mut raw = load_raw_config_with(RawConfigPathOptions::new(
        options.config_path,
        options.project_root,
    ))?;
    let version = raw.schema_version.unwrap_or(1);
    if version > 1 {
        anyhow::bail!("schema_version '{version}' is newer than this Berth build supports");
    }

    raw.schema_version = Some(1);
    let mut config = expansion::expand_raw_config(raw)?;
    let project_root = super::project::project_root_with(ProjectRootPathOptions::new(
        options.config_path,
        options.project_root,
    ))?;
    validation::validate_and_resolve_domains(&mut config, &project_root)?;
    validation::validate_and_resolve_container_names(&mut config)?;
    validation::validate_swarm_targets(&config)?;
    save_config_with(
        &config,
        SaveConfigPathOptions::new(options.config_path, options.project_root),
    )
}

#[cfg(test)]
mod tests {
    use super::{ProjectRootPathOptions, migrate_config_with};
    use crate::config::{LoadConfigPathOptions, load_config_with};
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_root() -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "berth-migrate-tests-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock")
                .as_nanos()
        ));
        drop(fs::remove_dir_all(&root));
        fs::create_dir_all(&root).expect("create temp root");
        root
    }

    fn write_config(root: &Path, content: &str) -> PathBuf {
        let path = root.join(".berth.toml");
        fs::write(&path, content).expect("write raw config");
        path
    }

    #[test]
    fn migrate_config_with_normalizes_legacy_schema_versions() -> anyhow::Result<()> {
        let root = temp_root();
        let path = write_config(
            &root,
            "project_type = \"project\"

[[service]]
name = \"app\"
kind = \"app\"
driver = \"frankenphp\"
image = \"nginx:1.29\"
container_name = \"app\"
",
        );

        migrate_config_with(ProjectRootPathOptions::new(Some(&path), None))?;
        let migrated = load_config_with(LoadConfigPathOptions::new(Some(&path), None))?;
        assert_eq!(migrated.schema_version, 1);
        assert_eq!(migrated.service[0].container_name.as_deref(), Some("app"));
        assert_eq!(
            migrated.service[0].resolved_container_name,
            Some("app".to_owned())
        );
        Ok(())
    }

    #[test]
    fn migrate_config_with_rejects_unknown_schema() {
        let root = temp_root();
        let path = write_config(
            &root,
            "schema_version = 2
project_type = \"project\"

[[service]]
name = \"app\"
kind = \"app\"
driver = \"frankenphp\"
image = \"nginx:1.29\"
container_name = \"app\"
",
        );

        let result = migrate_config_with(ProjectRootPathOptions::new(Some(&path), None));
        assert!(result.is_err());
    }

    #[test]
    fn migrate_config_with_preserves_domain_strategy_without_persisting_generated_domain()
    -> anyhow::Result<()> {
        let root = temp_root().join("my-project");
        fs::create_dir_all(&root).expect("create project root");
        let path = write_config(
            &root,
            "project_type = \"project\"
domain_strategy = \"directory\"
container_prefix = \"shipit-api\"

[[service]]
preset = \"laravel\"
",
        );

        migrate_config_with(ProjectRootPathOptions::new(Some(&path), None))?;

        let migrated_raw = fs::read_to_string(&path).expect("read migrated config");
        assert!(migrated_raw.contains("domain_strategy = \"directory\""));
        assert!(!migrated_raw.contains("domain = \"my-project.berth\""));

        let migrated = load_config_with(LoadConfigPathOptions::new(Some(&path), None))?;
        assert_eq!(migrated.service[0].domain, None);
        assert_eq!(
            migrated.service[0].primary_domain(),
            Some("my-project.berth")
        );

        Ok(())
    }
}
