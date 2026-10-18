//! Shared service startup helper for CLI handlers.

use anyhow::Result;
use std::collections::HashMap;
use std::path::Path;

use crate::{config, docker, serve};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WaitStrategy {
    Http,
    Container,
    Skip,
}

/// Immutable runtime context required to start services.
#[derive(Clone, Copy)]
pub(crate) struct ServiceStartContext<'a> {
    pub(crate) workspace_root: &'a Path,
    pub(crate) app_env: &'a HashMap<String, String>,
}

impl<'a> ServiceStartContext<'a> {
    pub(crate) fn new(workspace_root: &'a Path, app_env: &'a HashMap<String, String>) -> Self {
        Self {
            workspace_root,
            app_env,
        }
    }
}

fn wait_strategy_for_service(service: &config::ServiceConfig) -> WaitStrategy {
    if service.health_path.is_some() {
        return WaitStrategy::Http;
    }

    match service.driver {
        config::Driver::Horizon | config::Driver::Scheduler | config::Driver::Rabbitmq => {
            WaitStrategy::Container
        }
        _ => WaitStrategy::Skip,
    }
}

/// Whether waiting for the service checks its health. App services
/// without a health path or framework-boot probe are only started.
#[must_use]
pub(crate) fn is_health_checked(service: &config::ServiceConfig) -> bool {
    service.kind != config::Kind::App
        || wait_strategy_for_service(service) != WaitStrategy::Skip
        || (crate::env::is_laravel_app(service) && crate::gateway::is_routable(service))
}

/// Starts a service and optionally waits for health.
pub(crate) fn start_service(
    service: &config::ServiceConfig,
    context: &ServiceStartContext<'_>,
    recreate: bool,
    pull_policy: docker::PullPolicy,
    wait_healthy: bool,
    health_timeout_secs: Option<u64>,
    build: bool,
) -> Result<()> {
    if service.kind == config::Kind::App {
        serve::run(serve::RunServeOptions {
            target: service,
            recreate,
            trust_container_ca: service.trust_container_ca,
            detached: true,
            project_root: context.workspace_root,
            injected_env: context.app_env,
            allow_rebuild: build,
        })?;
        if wait_healthy {
            match wait_strategy_for_service(service) {
                WaitStrategy::Http => {
                    serve::wait_until_http_healthy(service, health_timeout_secs, 2, None)?;
                }
                WaitStrategy::Container => {
                    docker::wait_until_healthy(service, health_timeout_secs, 2, None)?;
                }
                WaitStrategy::Skip => {}
            }
            // A Laravel web app is ready only once the framework boots
            // (8.x FI-06); workers run `artisan` commands that fail loudly.
            if crate::env::is_laravel_app(service) && crate::gateway::is_routable(service) {
                serve::wait_until_laravel_boots(service, health_timeout_secs)?;
            }
        }
    } else {
        docker::up(service, pull_policy, recreate)?;
        if wait_healthy {
            docker::wait_until_healthy(service, health_timeout_secs, 2, None)?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{WaitStrategy, wait_strategy_for_service};
    use crate::config::{Driver, Kind, ServiceConfig};

    #[test]
    fn wait_strategy_uses_http_when_health_path_is_configured() {
        let mut service = service(Driver::Frankenphp);
        service.health_path = Some("/up".to_owned());

        assert_eq!(wait_strategy_for_service(&service), WaitStrategy::Http);
    }

    #[test]
    fn wait_strategy_uses_container_check_for_horizon_workers() {
        let service = service(Driver::Horizon);

        assert_eq!(wait_strategy_for_service(&service), WaitStrategy::Container);
    }

    #[test]
    fn wait_strategy_skips_queue_workers_without_http_or_container_health() {
        let mut service = service(Driver::Frankenphp);
        service.command = Some(vec![
            "php".to_owned(),
            "artisan".to_owned(),
            "queue:work".to_owned(),
        ]);

        assert_eq!(wait_strategy_for_service(&service), WaitStrategy::Skip);
    }

    fn service(driver: Driver) -> ServiceConfig {
        ServiceConfig {
            name: "app".to_owned(),
            kind: Kind::App,
            driver,
            image: "php".to_owned(),
            host: "127.0.0.1".to_owned(),
            port: 8080,
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
            container_name: Some("app".to_owned()),
            resolved_container_name: None,
            runtime: Default::default(),
        }
    }
}
