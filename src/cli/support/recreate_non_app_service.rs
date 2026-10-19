//! Shared non-app recreate helper for lifecycle handlers.

use anyhow::Result;

use crate::output::{self, LogLevel, Persistence};
use crate::{config, docker};

/// Recreates a non-app service, optionally starts it, and optionally waits.
pub(crate) fn recreate_non_app_service(
    service: &config::ServiceConfig,
    start_after_recreate: bool,
    start_pull_policy: docker::PullPolicy,
    wait_healthy: bool,
    health_timeout_secs: Option<u64>,
) -> Result<()> {
    docker::recreate(service)?;
    if start_after_recreate {
        docker::up(service, start_pull_policy, false)?;
    }
    if wait_healthy {
        docker::wait_until_healthy(service, health_timeout_secs, 2, None)?;
    }
    if start_after_recreate {
        emit_connection_string(service);
    }
    Ok(())
}

fn emit_connection_string(service: &config::ServiceConfig) {
    if !should_emit_connection_string(service) {
        return;
    }

    output::event(
        &service.name,
        LogLevel::Success,
        &format!(
            "Connection string: {}",
            crate::network::host_url(service)
                .unwrap_or_else(|| format!("({})", crate::network::PUBLISH_HINT))
        ),
        Persistence::Persistent,
    );
}

fn should_emit_connection_string(service: &config::ServiceConfig) -> bool {
    service.kind == config::Kind::Database
        || matches!(
            service.driver,
            config::Driver::Redis | config::Driver::Valkey
        )
}

#[cfg(test)]
mod tests {
    use crate::config::{Driver, Kind, ServiceConfig};

    use super::{emit_connection_string, recreate_non_app_service, should_emit_connection_string};

    fn service(name: &str) -> ServiceConfig {
        ServiceConfig {
            name: name.to_owned(),
            kind: Kind::Database,
            driver: Driver::Postgres,
            image: "postgres:16".to_owned(),
            host: "127.0.0.1".to_owned(),
            port: 5432,
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
            container_name: Some(name.to_owned()),
            resolved_container_name: Some(name.to_owned()),
            runtime: Default::default(),
        }
    }

    #[test]
    fn recreate_non_app_service_succeeds_without_start_or_wait() {
        crate::docker::with_dry_run_lock(|| {
            recreate_non_app_service(
                &service("db"),
                false,
                crate::docker::PullPolicy::Missing,
                false,
                None,
            )
            .expect("no-op path");
        });
    }

    #[test]
    fn recreate_non_app_service_starts_and_waits_when_requested() {
        crate::docker::with_dry_run_lock(|| {
            recreate_non_app_service(
                &service("db"),
                true,
                crate::docker::PullPolicy::Missing,
                true,
                None,
            )
            .expect("start and wait path");
        });
    }

    #[test]
    fn recreate_non_app_service_respects_start_pull_policy() {
        let _ = crate::docker::PullPolicy::Always;
        crate::docker::with_dry_run_lock(|| {
            recreate_non_app_service(
                &service("db-cache"),
                true,
                crate::docker::PullPolicy::Always,
                false,
                None,
            )
            .expect("always policy path");
        });
    }

    #[test]
    fn emit_connection_string_is_noop_for_unrelated_services() {
        let mut non_database = service("valkey");
        non_database.kind = Kind::Cache;
        non_database.driver = Driver::Memcached;
        emit_connection_string(&non_database);
    }

    #[test]
    fn should_emit_connection_string_for_database_redis_and_valkey() {
        let database = service("db");

        let mut redis = service("redis");
        redis.kind = Kind::Cache;
        redis.driver = Driver::Redis;

        let mut valkey = service("valkey");
        valkey.kind = Kind::Cache;
        valkey.driver = Driver::Valkey;

        let mut memcached = service("memcached");
        memcached.kind = Kind::Cache;
        memcached.driver = Driver::Memcached;

        assert!(should_emit_connection_string(&database));
        assert!(should_emit_connection_string(&redis));
        assert!(should_emit_connection_string(&valkey));
        assert!(!should_emit_connection_string(&memcached));
    }
}
