//! High-level serve lifecycle orchestration.
//!
//! Coordinates runtime image resolution, container startup, optional CA trust,
//! and Caddy/hosts integration for externally reachable URLs.

use anyhow::Result;
use std::collections::HashMap;
use std::path::Path;

use crate::config::ServiceConfig;
use crate::config::{is_unspecified_port_allocation_host, normalize_host_for_port_allocation};
use crate::output::{self, LogLevel, Persistence};

use super::{
    configure_caddy, ensure_container_running, ensure_hosts_entry, remove_caddy_route,
    remove_container, resolve_caddy_ports, resolve_runtime_image, served_url, service_domain,
    trust_inner_container_ca,
};

pub struct RunServeOptions<'a> {
    pub target: &'a ServiceConfig,
    pub recreate: bool,
    pub trust_container_ca: bool,
    pub detached: bool,
    pub project_root: &'a Path,
    pub injected_env: &'a HashMap<String, String>,
    pub allow_rebuild: bool,
}

/// Starts a serve target and configures Caddy reverse proxy with local TLS.
///
/// # Errors
///
/// Returns an error if Docker or Caddy orchestration fails.
pub fn run(options: RunServeOptions<'_>) -> Result<()> {
    let ports = resolve_caddy_ports()?;
    let runtime_image = resolve_runtime_image(
        options.target,
        options.allow_rebuild,
        options.injected_env,
        options.project_root,
    )?;
    ensure_container_running(
        options.target,
        options.recreate,
        &runtime_image,
        options.project_root,
        options.injected_env,
    )?;
    if options.trust_container_ca {
        trust_inner_container_ca(options.target, options.detached)?;
    }
    let url = if should_use_local_url(options.target) {
        local_url(options.target)
    } else {
        configure_caddy(options.target, ports)?;
        ensure_hosts_entry(options.target)?;
        caddy_served_url(options.target, ports.https)?
    };
    output::event(
        &options.target.name,
        LogLevel::Success,
        &format!("Serve target available at {url}"),
        Persistence::Persistent,
    );
    Ok(())
}

/// Stops and removes a serve target container and removes its Caddy route.
///
/// # Errors
///
/// Returns an error if Docker or Caddy orchestration fails.
pub fn down(target: &ServiceConfig) -> Result<()> {
    remove_container(target)?;
    if should_use_caddy(target) {
        remove_caddy_route(target)?;
    }
    Ok(())
}

/// Computes the public URL for a serve target based on configured Caddy ports.
///
/// # Errors
///
/// Returns an error if Caddy port env values are invalid.
pub fn public_url(target: &ServiceConfig) -> Result<String> {
    if should_use_local_url(target) {
        return Ok(local_url(target));
    }
    let ports = resolve_caddy_ports()?;
    caddy_served_url(target, ports.https)
}

fn localhost_url(target: &ServiceConfig) -> String {
    format!("https://localhost:{}", target.port)
}

fn should_use_local_url(target: &ServiceConfig) -> bool {
    target.localhost_tls || target.primary_domain().is_none()
}

fn should_use_caddy(target: &ServiceConfig) -> bool {
    !should_use_local_url(target)
}

fn local_url(target: &ServiceConfig) -> String {
    if target.localhost_tls {
        return localhost_url(target);
    }

    format!(
        "{}://{}:{}",
        target.scheme(),
        local_bind_host(&target.host),
        target.port
    )
}

fn local_bind_host(host: &str) -> String {
    if is_unspecified_port_allocation_host(host) {
        return "127.0.0.1".to_owned();
    }

    normalize_host_for_port_allocation(host)
}

fn caddy_served_url(target: &ServiceConfig, https_port: u16) -> Result<String> {
    Ok(served_url(service_domain(target)?, https_port))
}

#[cfg(test)]
mod tests {
    use super::{public_url, should_use_caddy, should_use_local_url};
    use crate::config::{Driver, Kind, ServiceConfig};

    fn service() -> ServiceConfig {
        ServiceConfig {
            name: "app".to_owned(),
            kind: Kind::App,
            driver: Driver::Frankenphp,
            image: "dunglas/frankenphp:php8.5".to_owned(),
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
            domain: Some("app.berth".to_owned()),
            domains: None,
            resolved_domain: None,
            container_port: Some(80),
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
            resolved_container_name: Some("app".to_owned()),
            runtime: Default::default(),
        }
    }

    #[test]
    fn public_url_falls_back_to_local_endpoint_when_domain_is_missing() {
        let mut target = service();
        target.domain = None;

        let url = public_url(&target).expect("public url");
        assert_eq!(url, "http://127.0.0.1:8080");
    }

    #[test]
    fn public_url_prefers_localhost_tls_when_enabled() {
        let mut target = service();
        target.localhost_tls = true;

        let url = public_url(&target).expect("public url");
        assert_eq!(url, "https://localhost:8080");
    }

    #[test]
    fn domainless_targets_use_local_url_instead_of_caddy() {
        let mut target = service();
        target.domain = None;

        assert!(should_use_local_url(&target));
        assert!(!should_use_caddy(&target));
    }
}
