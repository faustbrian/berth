//! cli handlers recreate cmd random ports module.
//!
//! Contains cli handlers recreate cmd random ports logic used by Berth command workflows.

use anyhow::Result;
use std::path::Path;

#[cfg(test)]
use crate::cli;
use crate::cli::support::ServiceStartContext;
use crate::config;

use flow::run_recreate_flow;
use preflight::{PreparedRecreateRuntime, prepare_recreate_runtime};
use runtime::{RecreateRuntimeContext, recreate_runtime};

mod flow;
mod planning;
mod preflight;
mod runtime;

pub(super) struct RandomPortsRecreateOptions<'a> {
    pub(super) service: Option<&'a str>,
    pub(super) services: &'a [String],
    pub(super) kind: Option<config::Kind>,
    pub(super) profile: Option<&'a str>,
    pub(super) healthy: bool,
    pub(super) timeout: u64,
    pub(super) save_ports: bool,
    pub(super) env_output: bool,
    pub(super) quiet: bool,
    pub(super) runtime_env_name: Option<&'a str>,
    pub(super) config_path: Option<&'a Path>,
    pub(super) project_root: Option<&'a Path>,
    pub(super) parallel: usize,
    pub(super) workspace_root: &'a Path,
}

pub(super) fn handle_random_ports_recreate(
    config: &mut config::Config,
    options: RandomPortsRecreateOptions<'_>,
) -> Result<()> {
    let PreparedRecreateRuntime {
        planned,
        app_env,
        env_path,
    } = prepare_recreate_runtime(
        config,
        options.service,
        options.services,
        options.kind,
        options.profile,
        options.env_output,
        options.runtime_env_name,
        options.config_path,
        options.project_root,
    )?;

    let recreate_context = RecreateRuntimeContext::new(
        options.healthy,
        options.timeout,
        ServiceStartContext::new(options.workspace_root, &app_env),
        options.quiet,
    );
    run_recreate_flow(
        &planned,
        config,
        recreate_context,
        env_path.as_deref(),
        options.save_ports,
        options.quiet,
        options.config_path,
        options.project_root,
        options.parallel,
        recreate_runtime,
    )?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Config, Driver, Kind, ServiceConfig};
    use std::path::Path;

    #[test]
    fn apply_runtime_binding_updates_matching_service_port() {
        let mut runtime_config = Config {
            schema_version: 1,
            project_type: config::ProjectType::Project,
            container_prefix: None,
            domain_strategy: None,
            service: vec![service("valkey", 6379), service("app", 8080)],
            swarm: Vec::new(),
        };
        let runtime_service = service("valkey", 50031);

        cli::support::apply_runtime_binding(&mut runtime_config, &runtime_service)
            .expect("apply runtime");

        let updated = runtime_config
            .service
            .iter()
            .find(|service| service.name == "valkey")
            .expect("service exists");
        assert_eq!(updated.port, 50031);
    }

    #[test]
    fn handle_random_ports_recreate_allows_parallel_execution() {
        let mut config = Config {
            schema_version: 1,
            project_type: config::ProjectType::Project,
            container_prefix: None,
            domain_strategy: None,
            service: Vec::new(),
            swarm: Vec::new(),
        };

        let result = handle_random_ports_recreate(
            &mut config,
            RandomPortsRecreateOptions {
                service: None,
                services: &[],
                kind: None,
                profile: None,
                healthy: false,
                timeout: 30,
                save_ports: false,
                env_output: false,
                quiet: true,
                runtime_env_name: None,
                config_path: None,
                project_root: None,
                parallel: 2,
                workspace_root: Path::new("."),
            },
        );

        assert!(result.is_ok());
    }

    fn service(name: &str, port: u16) -> ServiceConfig {
        ServiceConfig {
            name: name.to_owned(),
            kind: if name == "app" {
                Kind::App
            } else {
                Kind::Cache
            },
            driver: if name == "app" {
                Driver::Frankenphp
            } else {
                Driver::Valkey
            },
            image: "test-image".to_owned(),
            host: "127.0.0.1".to_owned(),
            port,
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
            container_name: None,
            resolved_container_name: None,
        }
    }
}
