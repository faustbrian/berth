//! doctor app domain resolution checks.

use crate::cli::support::run_doctor::report;
use crate::{config, serve};

/// Checks domain resolution and reports actionable failures.
pub(in crate::cli::support::run_doctor::app) fn check_domain_resolution(
    target: &config::ServiceConfig,
    fix: bool,
) -> bool {
    let domains = target.resolved_domains();
    if domains.is_empty() || target.localhost_tls {
        return false;
    }

    if fix {
        if let Err(err) = serve::ensure_hosts_entry_for_domain(target) {
            report::error(&format!("Hosts fix failed for '{}': {}", target.name, err));
            return true;
        }
        report::success(&format!("Hosts entry ensured for {}", domains.join(", ")));
        return false;
    }

    let mut has_error = false;
    for domain in domains {
        if !serve::domain_resolves_to_loopback(domain) {
            report::error(&format!("{domain} does not resolve to localhost"));
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
    use crate::docker;

    use super::check_domain_resolution;

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
    fn check_domain_resolution_reports_loopback_resolution() {
        let target = service(Some("127.0.0.1"), false);
        assert!(!check_domain_resolution(&target, false));
    }

    #[test]
    fn check_domain_resolution_flags_unresolved_domain() {
        let target = service(Some("256.0.0.1"), false);
        assert!(check_domain_resolution(&target, false));
    }

    #[test]
    fn check_domain_resolution_uses_fix_path_when_requested() {
        docker::with_dry_run_state(true, || {
            let target = service(Some("256.0.0.1"), false);
            assert!(!check_domain_resolution(&target, true));
        });
    }
}
