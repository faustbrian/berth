//! cli support run doctor ports module.
//!
//! Contains cli support run doctor ports logic used by Berth command workflows.

use std::collections::HashMap;

use crate::config;

mod checks;

/// Checks port conflicts and reports actionable failures.
pub(super) fn check_port_conflicts(config: &config::Config) -> bool {
    let mut has_error = false;
    let mut used_ports: HashMap<(String, u16), &str> = HashMap::new();

    for svc in &config.service {
        let running = checks::service_is_running(svc);
        has_error |= checks::check_service_ports(svc, running, &mut used_ports);
    }

    has_error
}

#[cfg(test)]
mod tests {
    use crate::cli;
    use crate::config::{Config, Driver, Kind, ServiceConfig};
    use std::net::TcpListener;

    #[test]
    fn host_port_availability_detects_busy_port() {
        let (_listener, port) = bind_random_listener("127.0.0.1");

        assert!(!cli::support::is_port_available_strict("127.0.0.1", port));
    }

    #[test]
    fn host_port_availability_detects_free_port() {
        let (listener, port) = bind_random_listener("127.0.0.1");
        drop(listener);

        assert!(cli::support::is_port_available_strict("127.0.0.1", port));
    }

    #[test]
    fn host_port_availability_respects_host_instead_of_fallback() {
        let port = (0..32)
            .find_map(|_| {
                let candidate =
                    cli::support::random_free_port("127.0.0.1").expect("allocate free port");
                if cli::support::is_port_available("192.0.2.1", candidate) {
                    Some(candidate)
                } else {
                    None
                }
            })
            .expect("allocate free port not occupied on fallback host");
        assert!(!cli::support::is_port_available_strict("192.0.2.1", port));
        assert!(cli::support::is_port_available("192.0.2.1", port));
    }

    #[test]
    fn check_port_conflicts_detects_primary_port_collision() {
        let config = Config {
            schema_version: 1,
            project_type: crate::config::ProjectType::Project,
            container_prefix: None,
            domain_strategy: None,
            service: vec![
                service(
                    "app",
                    Kind::App,
                    Driver::Frankenphp,
                    "127.0.0.1",
                    3000,
                    None,
                ),
                service(
                    "cache",
                    Kind::Cache,
                    Driver::Valkey,
                    "127.0.0.1",
                    3000,
                    None,
                ),
            ],
            swarm: Vec::new(),
        };

        assert!(super::check_port_conflicts(&config));
    }

    #[test]
    fn check_port_conflicts_detects_smtp_port_collision() {
        let config = Config {
            schema_version: 1,
            project_type: crate::config::ProjectType::Project,
            container_prefix: None,
            domain_strategy: None,
            service: vec![
                service(
                    "mailhog",
                    Kind::Cache,
                    Driver::Mailhog,
                    "127.0.0.1",
                    25_000,
                    Some(25001),
                ),
                service(
                    "worker",
                    Kind::Cache,
                    Driver::Valkey,
                    "127.0.0.1",
                    25001,
                    None,
                ),
            ],
            swarm: Vec::new(),
        };

        assert!(super::check_port_conflicts(&config));
    }

    #[test]
    fn check_port_conflicts_flags_in_use_smtp_port_for_stopped_service() {
        let (_listener, port) = bind_random_listener("127.0.0.1");
        let config = Config {
            schema_version: 1,
            project_type: crate::config::ProjectType::Project,
            container_prefix: None,
            domain_strategy: None,
            service: vec![service(
                "mailhog",
                Kind::Cache,
                Driver::Mailhog,
                "127.0.0.1",
                25_001,
                Some(port),
            )],
            swarm: Vec::new(),
        };

        assert!(super::check_port_conflicts(&config));
    }

    fn bind_random_listener(host: &str) -> (TcpListener, u16) {
        let listener = TcpListener::bind((host, 0)).expect("bind random port");
        let port = listener.local_addr().expect("local addr").port();
        (listener, port)
    }

    fn service(
        name: &str,
        kind: Kind,
        driver: Driver,
        host: &str,
        port: u16,
        smtp_port: Option<u16>,
    ) -> ServiceConfig {
        ServiceConfig {
            name: name.to_owned(),
            kind,
            driver,
            image: "test-image".to_owned(),
            host: host.to_owned(),
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
            routes: None,
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
}
