//! doctor app route resolution checks.

use crate::cli::support::run_doctor::report;
use crate::config;

/// Checks that each route resolves to loopback. Berth never edits
/// `/etc/hosts`; `*.localhost` must resolve on its own, so there is nothing
/// for `--fix` to repair here.
pub(in crate::cli::support::run_doctor::app) fn check_domain_resolution(
    target: &config::ServiceConfig,
    _fix: bool,
) -> bool {
    check_with(target, crate::gateway::resolves_to_loopback)
}

/// The check with an injected resolver, so tests never depend on the host's
/// DNS setup (8.x FI-25).
fn check_with(target: &config::ServiceConfig, resolves: impl Fn(&str) -> bool) -> bool {
    let domains = crate::gateway::route_hosts(target);
    if domains.is_empty() {
        return false;
    }

    let mut has_error = false;
    for domain in domains {
        if !resolves(&domain) {
            report::error(&format!(
                "{domain} does not resolve to loopback; this system's resolver must map \
                 *.localhost to 127.0.0.1 (RFC 6761)"
            ));
            has_error = true;
            continue;
        }

        report::success(&format!("{domain} resolves to localhost"));
    }
    has_error
}

#[cfg(test)]
mod tests {
    use crate::config::{Driver, Kind, ServiceConfig};

    use super::{check_domain_resolution, check_with};

    fn service(domain: Option<&str>, localhost_tls: bool) -> ServiceConfig {
        ServiceConfig {
            name: "app".to_owned(),
            kind: Kind::App,
            driver: Driver::Frankenphp,
            image: "dunglas/frankenphp:php8.5".to_owned(),
            host: "127.0.0.1".to_owned(),
            port: 3000,
            database: None,
            username: None,
            password: None,
            bucket: None,
            access_key: None,
            secret_key: None,
            api_key: None,
            region: None,
            scheme: None,
            domain: domain.map(ToOwned::to_owned),
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
            localhost_tls,
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
    fn check_domain_resolution_ignores_localhost_tls_target() {
        let target = service(Some("example.invalid"), true);
        assert!(!check_domain_resolution(&target, false));
    }

    #[test]
    fn routes_pass_when_they_resolve_to_loopback_and_fail_otherwise() {
        let mut target = service(None, false);
        target.resolved_domain = Some("a-1-shop.berth.localhost".to_owned());
        target.runtime.gateway_https_port = Some(8443);
        assert!(!check_with(&target, |_| true));
        assert!(check_with(&target, |_| false));
    }
}
