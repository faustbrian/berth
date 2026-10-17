//! database setup module.
//!
//! Contains database setup logic used by Berth command workflows.

use anyhow::{Context, Result};

use crate::config::ServiceConfig;
use crate::docker::PullPolicy;
use crate::output::{self, LogLevel, Persistence};

use super::sql_admin::create_database;

/// Ensures sql service exists and is in the required state.
pub(super) fn ensure_sql_service(service: &ServiceConfig) -> Result<()> {
    ensure_service_matches(
        service,
        ServiceCheck::Database,
        "service '{}' is not a SQL database",
    )
}

/// Ensures SQL service supports dump/restore operations.
pub(super) fn ensure_sql_dump_service(service: &ServiceConfig) -> Result<()> {
    ensure_service_matches(
        service,
        ServiceCheck::SupportsDump,
        "service '{}' does not support SQL dump/restore operations",
    )
}

pub(crate) fn setup(service: &ServiceConfig, timeout: u64) -> Result<()> {
    crate::docker::up(service, PullPolicy::Missing, false)
        .context("Failed to start service container")?;

    crate::docker::wait_until_healthy(service, timeout, 2, None)?;

    if service.supports_sql_dump() {
        create_database(service)?;
    }

    output::event(
        &service.name,
        LogLevel::Success,
        "Service is ready",
        Persistence::Persistent,
    );
    Ok(())
}

enum ServiceCheck {
    Database,
    SupportsDump,
}

fn ensure_service_matches(
    service: &ServiceConfig,
    check: ServiceCheck,
    error_message: &str,
) -> Result<()> {
    let is_valid = match check {
        ServiceCheck::Database => service.is_database(),
        ServiceCheck::SupportsDump => service.supports_sql_dump(),
    };
    if is_valid {
        return Ok(());
    }
    anyhow::bail!("{}", error_message.replace("{}", &service.name))
}

#[cfg(test)]
mod tests {
    use super::{ensure_sql_dump_service, ensure_sql_service, setup};
    use crate::config::{Driver, Kind, ServiceConfig};

    fn service(driver: Driver) -> ServiceConfig {
        ServiceConfig {
            name: "db".to_owned(),
            kind: Kind::Database,
            driver,
            image: "mysql:8.4".to_owned(),
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
            container_name: Some("db".to_owned()),
            resolved_container_name: Some("db".to_owned()),
        }
    }

    #[test]
    fn ensure_sql_service_accepts_database_driver_and_rejects_cache() {
        assert!(ensure_sql_service(&service(Driver::Mysql)).is_ok());
        assert!(ensure_sql_service(&service(Driver::Frankenphp)).is_err());
    }

    #[test]
    fn ensure_sql_dump_service_rejects_non_dump_backends() {
        assert!(ensure_sql_dump_service(&service(Driver::Postgres)).is_ok());
        assert!(ensure_sql_dump_service(&service(Driver::Redis)).is_err());
    }

    #[test]
    fn setup_uses_dry_run_path_without_create_database_for_non_dump_services() {
        let service = service(Driver::Redis);
        crate::docker::with_dry_run_lock(|| {
            setup(&service, 10).expect("setup should succeed in dry-run");
        });
    }

    #[test]
    fn setup_uses_dry_run_path_and_skips_create_database_for_dump_backends() {
        let service = service(Driver::Mysql);
        crate::docker::with_dry_run_lock(|| {
            setup(&service, 10).expect("setup should succeed in dry-run");
        });
    }
}
