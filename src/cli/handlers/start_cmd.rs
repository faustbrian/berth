//! cli handlers start cmd module.
//!
//! Contains cli handlers start cmd logic used by Berth command workflows.

use anyhow::Result;
use std::path::Path;

use crate::config;
use crate::docker;

mod flow;
mod open_after_start;
mod options;
mod start_bootstrap;
use flow::{StartFlowOptions, run_start_flow};
use open_after_start::maybe_open_after_start;
use options::resolve_start_wait_flags;

pub(crate) struct HandleStartOptions<'a> {
    pub(crate) service: Option<&'a str>,
    pub(crate) kind: Option<config::Kind>,
    pub(crate) profile: Option<&'a str>,
    pub(crate) wait: bool,
    pub(crate) no_wait: bool,
    pub(crate) wait_timeout: u64,
    pub(crate) pull_policy: docker::PullPolicy,
    pub(crate) force_recreate: bool,
    pub(crate) open_after_start: bool,
    pub(crate) health_path: Option<&'a str>,
    pub(crate) include_project_deps: bool,
    pub(crate) parallel: usize,
    pub(crate) quiet: bool,
    pub(crate) no_color: bool,
    pub(crate) dry_run: bool,
    pub(crate) repro: bool,
    pub(crate) runtime_env: Option<&'a str>,
    pub(crate) config_path: Option<&'a Path>,
    pub(crate) project_root: Option<&'a Path>,
}

pub(crate) fn handle_start(
    config: &mut config::Config,
    options: HandleStartOptions<'_>,
) -> Result<()> {
    let (effective_wait, effective_no_wait) =
        resolve_start_wait_flags(options.wait, options.no_wait);

    run_start_flow(
        config,
        StartFlowOptions {
            service: options.service,
            kind: options.kind,
            profile: options.profile,
            wait: effective_wait,
            no_wait: effective_no_wait,
            wait_timeout: options.wait_timeout,
            pull_policy: options.pull_policy,
            force_recreate: options.force_recreate,
            include_project_deps: options.include_project_deps,
            parallel: options.parallel,
            quiet: options.quiet,
            no_color: options.no_color,
            dry_run: options.dry_run,
            repro: options.repro,
            runtime_env: options.runtime_env,
            config_path: options.config_path,
            project_root: options.project_root,
        },
    )?;

    maybe_open_after_start(
        config,
        options.service,
        options.kind,
        options.open_after_start,
        options.health_path,
        options.quiet,
    )
}

#[cfg(test)]
mod tests {
    use super::{resolve_start_wait_flags, start_bootstrap};
    use crate::config::{Driver, Kind, ServiceConfig};
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn start_wait_defaults_to_no_wait_when_unspecified() {
        assert_eq!(resolve_start_wait_flags(false, false), (false, true));
    }

    #[test]
    fn start_wait_flag_enables_wait() {
        assert_eq!(resolve_start_wait_flags(true, false), (true, false));
    }

    #[test]
    fn start_no_wait_stays_no_wait() {
        assert_eq!(resolve_start_wait_flags(false, true), (false, true));
    }

    #[test]
    fn app_key_missing_returns_false_when_key_present() {
        let path = temp_env_path("present");
        fs::write(&path, "APP_NAME=Berth\nAPP_KEY=base64:abc123\n").expect("write env");

        assert!(!start_bootstrap::app_key_missing(&path));

        fs::remove_file(path).expect("cleanup env");
    }

    #[test]
    fn app_key_missing_returns_true_when_key_blank() {
        let path = temp_env_path("blank");
        fs::write(&path, "APP_KEY=\n").expect("write env");

        assert!(start_bootstrap::app_key_missing(&path));

        fs::remove_file(path).expect("cleanup env");
    }

    fn temp_env_path(suffix: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        std::env::temp_dir().join(format!("berth-start-{suffix}-{nanos}.env"))
    }

    #[test]
    fn select_bootstrap_target_prefers_primary_web_app() {
        let horizon = service("horizon", Driver::Horizon, Some(vec!["php".to_owned()]));
        let queue_worker = service(
            "queue-worker",
            Driver::Frankenphp,
            Some(vec!["php".to_owned(), "artisan".to_owned()]),
        );
        let app = service("app", Driver::Frankenphp, None);
        let selected = vec![&horizon, &queue_worker, &app];

        let target = start_bootstrap::select_bootstrap_target(&selected).expect("bootstrap target");
        assert_eq!(target.name, "app");
    }

    fn service(name: &str, driver: Driver, command: Option<Vec<String>>) -> ServiceConfig {
        ServiceConfig {
            name: name.to_owned(),
            kind: Kind::App,
            driver,
            image: "dunglas/frankenphp:php8.5".to_owned(),
            host: "127.0.0.1".to_owned(),
            port: 33065,
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
            command,
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
            container_name: None,
            resolved_container_name: None,
        }
    }
}
