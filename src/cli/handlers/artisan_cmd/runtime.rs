//! cli handlers artisan cmd runtime module.
//!
//! Contains cli handlers artisan cmd runtime logic used by Berth command workflows.

use anyhow::Result;
use std::path::Path;

use crate::{cli, config, serve};

mod cleanup_stale_runtime;
mod reset_service;
mod runtime_ports;
mod startup;
mod testing_runtime_env;
use cleanup_stale_runtime::cleanup_stale_testing_runtime_containers;
use reset_service::reset_service_runtime;
use runtime_ports::prepare_testing_runtime;
use startup::{resolve_testing_startup_services, run_testing_startup_services};
use testing_runtime_env::TestingRuntimeLease;

pub(super) fn acquire_testing_runtime_lease(workspace_root: &Path) -> Result<TestingRuntimeLease> {
    testing_runtime_env::acquire_testing_runtime_lease(workspace_root)
}

pub(super) fn set_testing_runtime_pool_size_override(pool_size: Option<usize>) {
    testing_runtime_env::set_testing_runtime_pool_size_override(pool_size);
}

/// Ensures test services running exists and is in the required state.
pub(super) fn ensure_test_services_running(
    config: &mut config::Config,
    workspace_root: &Path,
    selected_service: Option<&str>,
    bootstrap_playwright: bool,
) -> Result<()> {
    let (_, mut app_env) = prepare_testing_runtime(config)?;
    if bootstrap_playwright {
        let frankenphp_services = config
            .service
            .iter()
            .filter(|service| {
                service.kind == config::Kind::App && service.driver == config::Driver::Frankenphp
            })
            .map(|service| service.name.clone())
            .collect::<Vec<_>>();

        serve::enable_browser_test_runtime(&mut app_env, &frankenphp_services);
    }
    let startup_services = resolve_testing_startup_services(config, selected_service)?;
    cleanup_stale_testing_runtime_containers(&startup_services)?;
    let start_context = cli::support::ServiceStartContext::new(workspace_root, &app_env);
    run_testing_startup_services(&startup_services, &start_context, reset_service_runtime)
}

/// Tears down all test runtime services for the current runtime environment.
pub(super) fn cleanup_test_services(
    config: &config::Config,
    selected_service: Option<&str>,
) -> Result<()> {
    let startup_services = resolve_testing_startup_services(config, selected_service)?;
    for svc in &startup_services {
        reset_service_runtime(svc)?;
    }
    crate::network::remove_unused_networks(startup_services.iter().copied())?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::runtime_ports::{assign_testing_runtime_ports, prepare_testing_runtime};
    use crate::cli;
    use crate::config::{Config, Driver, Kind, ServiceConfig};

    #[test]
    fn assigns_random_service_port_for_testing_runtime() {
        let configured_port = cli::support::random_free_port("127.0.0.1").expect("config port");
        let mut config = Config {
            schema_version: 1,
            project_type: crate::config::ProjectType::Project,
            container_prefix: None,
            domain_strategy: None,
            service: vec![service("valkey", configured_port, None)],
            swarm: Vec::new(),
        };

        let remapped = assign_testing_runtime_ports(&mut config).expect("assign ports");
        let remapped_service = config.service.first().expect("service exists");

        assert!(remapped.contains("valkey"));
        assert_ne!(remapped_service.port, configured_port);
    }

    #[test]
    fn assigns_random_smtp_port_for_testing_runtime() {
        let app_port = cli::support::random_free_port("127.0.0.1").expect("app test port");
        let smtp_port = cli::support::random_free_port("127.0.0.1").expect("smtp test port");
        let mut config = Config {
            schema_version: 1,
            project_type: crate::config::ProjectType::Project,
            container_prefix: None,
            domain_strategy: None,
            service: vec![service("mailhog", app_port, Some(smtp_port))],
            swarm: Vec::new(),
        };

        let remapped = assign_testing_runtime_ports(&mut config).expect("assign ports");
        let remapped_service = config.service.first().expect("service exists");

        assert!(remapped.contains("mailhog"));
        assert_ne!(remapped_service.smtp_port, Some(smtp_port));
    }

    #[test]
    fn forces_app_services_to_localhost_tls_for_testing_runtime_isolation() {
        let app_port = cli::support::random_free_port("127.0.0.1").expect("app test port");
        let mut config = Config {
            schema_version: 1,
            project_type: crate::config::ProjectType::Project,
            container_prefix: None,
            domain_strategy: None,
            service: vec![app_service("app", app_port)],
            swarm: Vec::new(),
        };

        assign_testing_runtime_ports(&mut config).expect("assign ports");
        let app = config.service.first().expect("service exists");

        assert!(app.localhost_tls);
    }

    #[test]
    fn clears_app_service_domains_for_testing_runtime_isolation() {
        let app_port = cli::support::random_free_port("127.0.0.1").expect("app test port");
        let mut config = Config {
            schema_version: 1,
            project_type: crate::config::ProjectType::Project,
            container_prefix: None,
            domain_strategy: None,
            service: vec![app_service("app", app_port)],
            swarm: Vec::new(),
        };
        config.service[0].domain = Some("acme-app.localhost".to_owned());
        config.service[0].domains = Some(vec!["alt.localhost".to_owned()]);

        assign_testing_runtime_ports(&mut config).expect("assign ports");
        let app = config.service.first().expect("service exists");

        assert_eq!(app.domain, None);
        assert_eq!(app.domains, None);
    }

    #[test]
    fn builds_testing_env_after_port_remapping() {
        let app_port = cli::support::random_free_port("127.0.0.1").expect("app test port");
        let cache_port = cli::support::random_free_port("127.0.0.1").expect("cache test port");
        let mut config = Config {
            schema_version: 1,
            project_type: crate::config::ProjectType::Project,
            container_prefix: None,
            domain_strategy: None,
            service: vec![
                app_service("app", app_port),
                service("redis", cache_port, None),
            ],
            swarm: Vec::new(),
        };

        let (_, app_env) = prepare_testing_runtime(&mut config).expect("prepare runtime");
        let remapped_cache = config
            .service
            .iter()
            .find(|svc| svc.name == "redis")
            .expect("cache service");

        assert_eq!(
            app_env.get("REDIS_PORT"),
            Some(&remapped_cache.port.to_string())
        );
    }

    fn service(name: &str, port: u16, smtp_port: Option<u16>) -> ServiceConfig {
        ServiceConfig {
            name: name.to_owned(),
            kind: Kind::Cache,
            driver: Driver::Valkey,
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
            smtp_port,
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
            runtime: Default::default(),
        }
    }

    fn app_service(name: &str, port: u16) -> ServiceConfig {
        ServiceConfig {
            kind: Kind::App,
            driver: Driver::Frankenphp,
            ..service(name, port, None)
        }
    }
}
