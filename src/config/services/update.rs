//! config services update module.
//!
//! Contains config services update logic used by Berth command workflows.

use anyhow::Result;

use crate::config::Config;

/// Updates service port to persisted or external state.
pub(crate) fn update_service_port(config: &mut Config, name: &str, port: u16) -> Result<()> {
    let service = config
        .service
        .iter_mut()
        .find(|svc| svc.name == name)
        .ok_or_else(|| anyhow::anyhow!("service '{name}' not found while updating port"))?;
    service.port = port;
    Ok(())
}

/// Updates service host port to persisted or external state.
pub(crate) fn update_service_host_port(
    config: &mut Config,
    name: &str,
    host: &str,
    port: u16,
) -> Result<bool> {
    let service = config
        .service
        .iter_mut()
        .find(|svc| svc.name == name)
        .ok_or_else(|| anyhow::anyhow!("service '{name}' not found while updating endpoint"))?;

    let changed = service.host != host || service.port != port;
    if changed {
        service.host = host.to_owned();
        service.port = port;
    }
    Ok(changed)
}

#[cfg(test)]
mod tests {
    use crate::config::{Config, Kind::App, Kind::Database, ServiceConfig};

    use super::{update_service_host_port, update_service_port};

    fn base_config() -> Config {
        Config {
            schema_version: 1,
            project_type: crate::config::ProjectType::Project,
            container_prefix: None,
            domain_strategy: None,
            service: vec![mysql_service("db"), app_service("web")],
            swarm: Vec::new(),
        }
    }

    fn mysql_service(name: &str) -> ServiceConfig {
        ServiceConfig {
            name: name.to_owned(),
            kind: Database,
            driver: crate::config::Driver::Mysql,
            image: "mysql:8.4".to_owned(),
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

    fn app_service(name: &str) -> ServiceConfig {
        ServiceConfig {
            name: name.to_owned(),
            kind: App,
            driver: crate::config::Driver::Frankenphp,
            image: "dunglas/frankenphp:php8.5".to_owned(),
            host: "127.0.0.1".to_owned(),
            port: 8000,
            database: None,
            username: None,
            password: None,
            bucket: None,
            access_key: None,
            secret_key: None,
            api_key: None,
            region: None,
            scheme: None,
            domain: Some("app.localhost".to_owned()),
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
            container_name: Some("acme-web".to_owned()),
            resolved_container_name: None,
        }
    }

    #[test]
    fn update_service_port_updates_existing_service() {
        let mut config = base_config();
        update_service_port(&mut config, "db", 5432).expect("update existing service port");
        assert_eq!(config.service[0].port, 5432);
    }

    #[test]
    fn update_service_port_fails_when_service_missing() {
        let mut config = base_config();
        let result = update_service_port(&mut config, "missing", 5432);
        assert!(result.is_err());
    }

    #[test]
    fn update_service_host_port_only_updates_when_values_change() {
        let mut config = base_config();

        let unchanged = update_service_host_port(&mut config, "db", "127.0.0.1", 3306)
            .expect("check existing values");
        assert!(!unchanged);
        assert_eq!(config.service[0].host, "127.0.0.1");
        assert_eq!(config.service[0].port, 3306);

        let changed = update_service_host_port(&mut config, "db", "127.0.0.2", 3307)
            .expect("apply new host and port");
        assert!(changed);
        assert_eq!(config.service[0].host, "127.0.0.2");
        assert_eq!(config.service[0].port, 3307);
    }
}
