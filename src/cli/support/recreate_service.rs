//! Shared service recreate helper for lifecycle handlers.

use anyhow::Result;

use crate::{config, docker};

/// Recreates a service and optionally waits for health.
///
/// App services are recreated through the app startup path so label and runtime
/// behavior stays aligned with `up`/`serve`. Non-app services use Docker-native
/// recreate/start/wait flow.
pub(crate) fn recreate_service(
    service: &config::ServiceConfig,
    start_context: &super::ServiceStartContext<'_>,
    wait_healthy: bool,
    health_timeout_secs: Option<u64>,
) -> Result<()> {
    if service.kind == config::Kind::App {
        return super::start_service(
            service,
            start_context,
            true,
            docker::PullPolicy::Missing,
            wait_healthy,
            health_timeout_secs,
            true,
        );
    }

    super::recreate_non_app_service(
        service,
        true,
        docker::PullPolicy::Missing,
        wait_healthy,
        health_timeout_secs,
    )
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::path::Path;

    use crate::config::{Driver, Kind, ServiceConfig};

    use super::recreate_service;
    use crate::cli::support::ServiceStartContext;

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
    fn recreate_service_delegates_non_app_services_to_non_app_path() {
        let service = service("db");
        let env = HashMap::new();
        let context = ServiceStartContext::new(Path::new("/tmp"), &env);
        crate::docker::with_dry_run_lock(|| {
            recreate_service(&service, &context, false, Some(30)).expect("non-app recreate");
        });
    }
}
