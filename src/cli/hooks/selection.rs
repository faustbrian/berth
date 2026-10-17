//! Hook execution helpers for common service-selection patterns.

use anyhow::Result;
use std::path::Path;

use crate::{cli, config};

pub(crate) fn run_hooks_for_up_selection<'a>(
    config: &'a config::Config,
    service: Option<&'a str>,
    kind: Option<config::Kind>,
    profile: Option<&'a str>,
    phase: config::HookPhase,
    workspace_root: &Path,
    quiet: bool,
) -> Result<Vec<&'a config::ServiceConfig>> {
    let selected = cli::support::resolve_up_services(config, service, kind, profile)?;
    super::run_phase_hooks_for_services(&selected, phase, workspace_root, quiet)?;
    Ok(selected)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use crate::config::{Config, Driver, HookPhase, Kind, ServiceConfig};

    use super::run_hooks_for_up_selection;

    fn service(name: &str, kind: Kind, driver: Driver) -> ServiceConfig {
        ServiceConfig {
            name: name.to_owned(),
            kind,
            driver,
            image: "dunglas/frankenphp:php8.5".to_owned(),
            host: "127.0.0.1".to_owned(),
            port: 9000,
            database: None,
            username: None,
            password: None,
            bucket: None,
            access_key: None,
            secret_key: None,
            api_key: None,
            region: None,
            scheme: None,
            domain: None,
            domains: None,
            resolved_domain: None,
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
            resolved_container_name: Some(format!("{name}-container")),
            runtime: Default::default(),
        }
    }

    fn config() -> Config {
        Config {
            schema_version: 1,
            project_type: crate::config::ProjectType::Project,
            container_prefix: None,
            domain_strategy: None,
            service: vec![
                service("app", Kind::App, Driver::Frankenphp),
                service("db", Kind::Database, Driver::Postgres),
            ],
            swarm: Vec::new(),
        }
    }

    #[test]
    fn run_hooks_for_up_selection_resolves_up_services() {
        let cfg = config();
        let services = run_hooks_for_up_selection(
            &cfg,
            None,
            Some(Kind::Database),
            None,
            HookPhase::PostUp,
            Path::new("/tmp/workspace"),
            true,
        )
        .expect("up selection");

        assert_eq!(services[0].name, "db");
        assert_eq!(services.len(), 1);
    }
}
