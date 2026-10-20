//! cli handlers config cmd module.
//!
//! Contains cli handlers config cmd logic used by Berth command workflows.

use anyhow::Result;
use std::path::Path;

use crate::cli::handlers::{log, serialize};
use crate::config;

/// Handles the `config` CLI command.
pub(crate) fn handle_config(config: &config::Config, format: &str) -> Result<()> {
    serialize::print_pretty(config, format)
}

/// Handles `config validate`. Loading the configuration applied the schema,
/// name, preset, policy and unknown-`depends_on` checks; `depends_on` cycles
/// are found here exactly as `up` resolves them. Nothing touches Docker.
pub(crate) fn handle_config_validate(config: &config::Config, json: bool) -> Result<()> {
    crate::cli::support::resolve_up_services(config, None, None, None)?;
    let runtime = config.service.first().map(|service| &service.runtime);
    let project = runtime.and_then(|runtime| runtime.project.clone());
    let instance = runtime.and_then(|runtime| runtime.instance.clone());
    if json {
        let services: Vec<serde_json::Value> = config
            .service
            .iter()
            .map(|service| {
                serde_json::json!({
                    "name": service.name,
                    "kind": crate::docker::kind_label_value(service.kind),
                    "shared": service.runtime.shared.is_some(),
                })
            })
            .collect();
        return serialize::print_json_pretty(&serde_json::json!({
            "valid": true,
            "project": project,
            "instance": instance,
            "services": services,
        }));
    }
    crate::output::event(
        "config",
        crate::output::LogLevel::Success,
        &format!(
            "Configuration is valid ({} services, instance {})",
            config.service.len(),
            instance.as_deref().unwrap_or("none")
        ),
        crate::output::Persistence::Transient,
    );
    Ok(())
}

/// Handles the `config migrate` CLI command.
pub(crate) fn handle_config_migrate(
    quiet: bool,
    config_path: Option<&Path>,
    project_root: Option<&Path>,
) -> Result<()> {
    let migrated = config::migrate_config_with(config::MigrateConfigOptions {
        config_path,
        project_root,
        runtime_env: None,
        instance: None,
    })?;
    for file in migrated {
        log::info_if_not_quiet(
            quiet,
            "config",
            &format!(
                "Converted {} to {}",
                file.source.display(),
                file.target.display()
            ),
        );
        for note in file.notes {
            log::info_if_not_quiet(quiet, "config", &format!("  {note}"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    use crate::config::{Config, Driver, Kind, ServiceConfig};

    use super::{handle_config, handle_config_migrate};

    fn service(name: &str, kind: Kind, driver: Driver) -> ServiceConfig {
        ServiceConfig {
            name: name.to_owned(),
            kind,
            driver,
            image: "service:latest".to_owned(),
            host: "127.0.0.1".to_owned(),
            port: 3306,
            database: Some("app".to_owned()),
            username: Some("root".to_owned()),
            password: Some("secret".to_owned()),
            bucket: None,
            access_key: None,
            secret_key: None,
            api_key: None,
            region: None,
            scheme: None,
            domain: None,
            domains: None,
            resolved_domain: None,
            routes: None,
            container_port: None,
            smtp_port: None,
            volumes: None,
            env: None,
            command: None,
            depends_on: None,
            seed_file: None,
            hook: Vec::new(),
            health_path: None,
            health_statuses: None,
            restart: None,
            localhost_tls: false,
            octane: false,
            octane_workers: None,
            octane_max_requests: None,
            php_extensions: None,
            trust_container_ca: false,
            env_mapping: None,
            javascript: None,
            container_name: Some(format!("{name}-container")),
            resolved_container_name: None,
            runtime: Default::default(),
        }
    }

    fn base_config() -> Config {
        Config {
            schema_version: 1,
            project_type: crate::config::ProjectType::Project,
            container_prefix: Some("berth".to_owned()),
            domain_strategy: None,
            service: vec![service("app", Kind::App, Driver::Frankenphp)],
            swarm: Vec::new(),
        }
    }

    fn temp_root() -> std::path::PathBuf {
        let root = crate::test_support::temp_root().join(format!(
            "berth-config-cmd-tests-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock")
                .as_nanos()
        ));
        fs::create_dir_all(&root).expect("create temp dir");
        root
    }

    #[test]
    fn handle_config_renders_supported_formats() -> Result<()> {
        let cfg = base_config();
        handle_config(&cfg, "json")?;
        handle_config(&cfg, "toml")?;
        Ok(())
    }

    #[test]
    fn handle_config_rejects_unknown_format() {
        let cfg = base_config();
        assert!(handle_config(&cfg, "yaml").is_err());
    }

    #[test]
    fn handle_config_migrate_converts_the_legacy_file_beside_the_config_path() -> Result<()> {
        let root = temp_root();
        let legacy_path = root.join(".stackctl.toml");
        fs::write(
            &legacy_path,
            r#"
schema_version = 1
project_type = "project"
container_prefix = "berth"

[[service]]
name = "app"
kind = "app"
driver = "frankenphp"
image = "nginx:1.29"
"#,
        )?;

        handle_config_migrate(false, Some(&legacy_path), None)?;
        let migrated = fs::read_to_string(root.join(".berth.yaml"))?;
        assert!(migrated.contains("schema_version: 9"), "{migrated}");
        assert!(migrated.contains("project: berth"), "{migrated}");
        Ok(())
    }

    #[test]
    fn handle_config_migrate_fails_without_config_path() {
        let root = temp_root();
        let result = handle_config_migrate(false, Some(&root.join("missing.toml")), None);
        assert!(result.is_err());
    }

    #[test]
    fn handle_config_migrate_rejects_a_config_path_that_is_not_a_legacy_project_file() {
        let root = temp_root();
        let error = handle_config_migrate(false, Some(&root.join("ci.toml")), None)
            .expect_err("arbitrary file names are not migrated");
        assert!(error.to_string().contains("ci.toml"), "{error}");
    }
}
