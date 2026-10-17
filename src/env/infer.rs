//! Runtime environment inference for app containers.
//!
//! This module derives Laravel-style env variables from configured backend
//! services and app targets, then applies precedence rules that preserve explicit
//! user-provided values.

use std::collections::HashMap;

use crate::config::{Config, Driver, Kind, ProjectType, ServiceConfig};

mod app_targets;
mod service_backends;

/// Builds inferred runtime environment values for an app container from configured services.
///
/// This map is intended for container runtime injection so loopback hosts use
/// the selected runtime host-gateway alias.
#[must_use]
pub(crate) fn inferred_app_env(config: &Config) -> HashMap<String, String> {
    let mut vars = HashMap::new();
    vars.insert(
        "BERTH_SQL_CLIENT_FLAVOR".to_owned(),
        crate::config::preferred_sql_client_flavor(config).to_owned(),
    );
    if config.project_type == ProjectType::Library {
        return vars;
    }

    for service in &config.service {
        if service.kind != Kind::App {
            service_backends::apply_service_env(&mut vars, service);
        }
    }

    for service in &config.service {
        if service.kind == Kind::App {
            app_targets::apply_app_target_env(&mut vars, service);
        }
    }

    vars
}

/// Builds environment values managed by app runtime injection:
/// inferred values plus explicit app target env overrides.
#[must_use]
pub(crate) fn managed_app_env(config: &Config) -> HashMap<String, String> {
    let mut vars = inferred_app_env(config);
    for service in &config.service {
        if service.kind != Kind::App {
            continue;
        }
        if let Some(explicit) = &service.env {
            for (key, value) in explicit {
                insert_if_absent(&mut vars, key, value.clone());
            }
        }
    }
    vars
}

/// Resolves the host value an app container can actually reach at runtime.
///
/// Why: `localhost` inside a container points to itself, not the host machine.
pub(super) fn runtime_host_for_app(service: &ServiceConfig) -> String {
    // On an instance network, services are reached by their service name.
    if service.runtime.network.is_some() {
        return service.name.clone();
    }
    if service.uses_host_gateway_alias() {
        return crate::docker::host_gateway_alias().to_owned();
    }
    service.host.clone()
}

/// Resolves the port an app container uses to reach a service: the
/// container port on an instance network, else the published host port.
pub(super) fn runtime_port_for_app(service: &ServiceConfig) -> u16 {
    if service.runtime.network.is_some() {
        return service.resolved_container_port();
    }
    service.port
}

/// Inserts a key/value pair only when the key is not already present.
///
/// Why: preserves explicit user-provided values over inferred defaults.
pub(super) fn insert_if_absent(vars: &mut HashMap<String, String>, key: &str, value: String) {
    if !vars.contains_key(key) {
        vars.insert(key.to_owned(), value);
    }
}

/// Derives the app's public URL used by app-facing env variables.
///
/// Prefers explicit localhost TLS URLs; otherwise uses the service primary domain.
pub(super) fn inferred_app_public_url(service: &ServiceConfig) -> Option<String> {
    if service.localhost_tls {
        return Some(format!("https://localhost:{}", service.port));
    }

    // Only web apps give the project its URL; a worker (the `queue-worker`
    // preset) listed first must never set `APP_URL`.
    if !crate::gateway::is_routable(service) {
        return None;
    }
    crate::gateway::route_urls(service)
        .into_iter()
        .next()
        .or_else(|| Some(crate::serve::local_url(service)))
}

/// Builds a canonical `scheme://host:port` endpoint for a service.
///
/// Why: backend env vars must use one stable endpoint format across drivers.
pub(super) fn service_endpoint(service: &ServiceConfig) -> String {
    format!(
        "{}://{}:{}",
        service.scheme(),
        runtime_host_for_app(service),
        runtime_port_for_app(service)
    )
}

/// Returns whether the service is an app target with the given runtime driver.
pub(super) fn is_app_driver(service: &ServiceConfig, driver: Driver) -> bool {
    service.kind == Kind::App && service.driver == driver
}
