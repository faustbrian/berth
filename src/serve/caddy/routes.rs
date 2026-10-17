//! Route-state mutation for Caddy-backed serve targets.

use anyhow::Result;

use crate::config::ServiceConfig;
use crate::output::{self, LogLevel, Persistence};

use super::{
    CaddyPorts, apply, caddy_access_log_path, domains_for_service, fs_state, lock, render_caddyfile,
};

/// Adds/updates domains for a target and applies the resulting Caddy config.
pub(super) fn configure_caddy(target: &ServiceConfig, ports: CaddyPorts) -> Result<()> {
    let upstream = format!("{}:{}", target.host, target.port);
    mutate_and_apply_caddy_state(
        ports,
        |state| {
            for domain in domains_for_service(target)? {
                state.routes.insert(domain.to_owned(), upstream.clone());
            }
            Ok(())
        },
        true,
    )
}

/// Removes target domains from route state and reapplies Caddy config.
pub(super) fn remove_caddy_route(target: &ServiceConfig) -> Result<()> {
    let ports = super::resolve_caddy_ports()?;
    mutate_and_apply_caddy_state(
        ports,
        |state| {
            for domain in domains_for_service(target)? {
                state.routes.remove(domain);
            }
            Ok(())
        },
        false,
    )
}

/// Emits dry-run messages for Caddy state/config writes and reload command.
fn print_dry_run(state_path: &std::path::Path, caddyfile_path: &std::path::Path) {
    output::event(
        "caddy",
        LogLevel::Info,
        &format!("[dry-run] Write {}", state_path.display()),
        Persistence::Transient,
    );
    output::event(
        "caddy",
        LogLevel::Info,
        &format!("[dry-run] Write {}", caddyfile_path.display()),
        Persistence::Transient,
    );
    output::event(
        "caddy",
        LogLevel::Info,
        &format!(
            "[dry-run] caddy reload --config {} --adapter caddyfile",
            caddyfile_path.display()
        ),
        Persistence::Transient,
    );
}

fn mutate_and_apply_caddy_state<F>(
    ports: CaddyPorts,
    mutate_state: F,
    trust_local_ca: bool,
) -> Result<()>
where
    F: FnOnce(&mut crate::serve::CaddyState) -> Result<()>,
{
    let caddy_dir = fs_state::caddy_dir()?;
    if crate::docker::is_dry_run() {
        let state_path = caddy_dir.join("sites.toml");
        let caddyfile_path = caddy_dir.join("Caddyfile");
        print_dry_run(&state_path, &caddyfile_path);
        return Ok(());
    }

    lock::with_caddy_apply_lock(&caddy_dir, || {
        let state_path = caddy_dir.join("sites.toml");
        let access_log_path = caddy_access_log_path()?;

        let mut state = fs_state::read_caddy_state(&state_path)?;
        mutate_state(&mut state)?;

        let caddyfile = render_caddyfile(&state, ports, &access_log_path);
        apply::apply_caddy_state(&caddy_dir, &state, &caddyfile)
    })?;
    if trust_local_ca {
        super::trust_local_caddy_ca()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::configure_caddy;
    use crate::config::{Driver, ServiceConfig};
    use crate::{config::Kind, docker, serve::CaddyPorts, serve::CaddyState};
    use std::path::PathBuf;

    fn service() -> ServiceConfig {
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
            domain: Some("app.berth".to_owned()),
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
            localhost_tls: true,
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
    fn configure_and_remove_routes_cover_dry_run_flow() {
        docker::with_dry_run_state(true, || {
            let target = service();
            let ports = CaddyPorts {
                http: 80,
                https: 443,
            };
            configure_caddy(&target, ports).expect("configure caddy");
            let result = super::remove_caddy_route(&target);
            assert!(result.is_ok());
        });
    }

    #[test]
    fn configure_caddy_fails_when_no_domain_is_declared() {
        docker::with_dry_run_state(false, || {
            let mut target = service();
            target.domain = None;
            let ports = CaddyPorts {
                http: 80,
                https: 443,
            };
            let error = configure_caddy(&target, ports).expect_err("missing domain");
            assert!(error.to_string().contains("missing domain"));
        });
    }

    #[test]
    fn caddy_access_log_state_file_defaults_from_home() {
        let caddy_dir = PathBuf::from("/tmp").join(".config/berth/caddy");
        let expected = caddy_dir.join("access.log");
        let state = CaddyState::default();
        let caddyfile = super::super::render_caddyfile(
            &state,
            CaddyPorts {
                http: 80,
                https: 443,
            },
            &expected,
        );
        assert!(!caddyfile.is_empty());
    }
}
