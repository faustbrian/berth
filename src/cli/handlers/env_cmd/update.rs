//! cli handlers env cmd update module.
//!
//! Contains cli handlers env cmd update logic used by Berth command workflows.

use anyhow::Result;
use std::path::Path;

use crate::cli::handlers::log;
use crate::{cli, config, env};

/// Handles the `service env update` CLI command.
pub(super) struct ServiceEnvUpdateOptions<'a> {
    pub(super) service: Option<&'a str>,
    pub(super) kind: Option<config::Kind>,
    pub(super) env_path: &'a Path,
    pub(super) create_missing: bool,
    pub(super) quiet: bool,
}

pub(super) fn handle_service_env_update(
    config: &mut config::Config,
    options: ServiceEnvUpdateOptions<'_>,
) -> Result<()> {
    if let Some(name) = options.service {
        let svc = config::find_service(config, name)?;
        if cli::support::matches_filter(svc, options.kind, None) {
            env::update_env(svc, options.env_path, options.create_missing)?;
            log::success_if_not_quiet(
                options.quiet,
                &svc.name,
                &format!("Updated {} with service config", options.env_path.display()),
            );
        }
        return Ok(());
    }

    for svc in cli::support::filter_services(&config.service, options.kind, None) {
        env::update_env(svc, options.env_path, options.create_missing)?;
        log::success_if_not_quiet(
            options.quiet,
            &svc.name,
            &format!("Updated {} with service config", options.env_path.display()),
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::config;
    use crate::docker;
    use std::fs;

    use super::{ServiceEnvUpdateOptions, handle_service_env_update};

    #[test]
    fn handle_service_env_update_updates_matching_service_values() -> anyhow::Result<()> {
        let mut mysql = config::preset_preview("mysql")?;
        mysql.name = "db".to_owned();
        let laravel = config::preset_preview("laravel")?;
        let mut config = config::Config {
            schema_version: 1,
            project_type: config::ProjectType::Project,
            container_prefix: Some("berth".to_owned()),
            domain_strategy: None,
            service: vec![laravel, mysql],
            swarm: Vec::new(),
        };

        let env_path = std::env::temp_dir().join(format!(
            "berth-env-update-{}.env",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_nanos()
        ));
        fs::write(&env_path, "DB_PASSWORD=\"old\"\n")?;
        docker::with_dry_run_state(false, || {
            handle_service_env_update(
                &mut config,
                ServiceEnvUpdateOptions {
                    service: Some("db"),
                    kind: None,
                    env_path: &env_path,
                    create_missing: false,
                    quiet: true,
                },
            )
        })?;

        let content = fs::read_to_string(&env_path)?;
        assert!(content.contains("DB_PASSWORD=\"laravel\""));
        assert_ne!(content, "DB_PASSWORD=\"old\"\n");
        Ok(())
    }

    #[test]
    fn handle_service_env_update_noop_when_service_is_missing() {
        let config = config::Config {
            schema_version: 1,
            project_type: config::ProjectType::Project,
            container_prefix: Some("berth".to_owned()),
            domain_strategy: None,
            service: Vec::new(),
            swarm: Vec::new(),
        };
        let env_path = std::env::temp_dir().join("berth-env-update-missing.env");

        let result = handle_service_env_update(
            &mut config.clone(),
            ServiceEnvUpdateOptions {
                service: Some("missing"),
                kind: None,
                env_path: &env_path,
                create_missing: false,
                quiet: true,
            },
        );
        assert!(result.is_err());
    }
}
