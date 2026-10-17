//! cli support ensure sql service module.
//!
//! Contains cli support ensure sql service logic used by Berth command workflows.

use anyhow::Result;

use crate::config;

/// Ensures sql service exists and is in the required state.
pub(crate) fn ensure_sql_service(service: &config::ServiceConfig, command: &str) -> Result<()> {
    if service.supports_sql_dump() {
        return Ok(());
    }

    anyhow::bail!(
        "'{}' supports SQL database services only. '{}' uses driver '{}'.",
        command,
        service.name,
        super::driver_name(service.driver)
    )
}

#[cfg(test)]
mod tests {
    use super::ensure_sql_service;
    use crate::config::{Driver, Kind, ServiceConfig};

    fn db_service(name: &str, driver: Driver) -> ServiceConfig {
        ServiceConfig {
            name: name.to_owned(),
            kind: Kind::Database,
            driver,
            image: "service:latest".to_owned(),
            host: "127.0.0.1".to_owned(),
            port: 3306,
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
        }
    }

    #[test]
    fn ensure_sql_service_accepts_supported_drivers() {
        assert!(ensure_sql_service(&db_service("mysql", Driver::Mysql), "dump").is_ok());
        assert!(ensure_sql_service(&db_service("postgres", Driver::Postgres), "dump").is_ok());
    }

    #[test]
    fn ensure_sql_service_rejects_unsupported_driver() {
        let err = ensure_sql_service(&db_service("redis", Driver::Redis), "status")
            .expect_err("unsupported");
        assert!(
            err.to_string()
                .contains("supports SQL database services only")
        );
        assert!(err.to_string().contains("redis"));
    }
}
