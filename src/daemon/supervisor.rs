//! Per-project daemon supervision loop.

use anyhow::Result;
use std::path::Path;
use std::thread;
use std::time::Duration;

use crate::cli::args::PortStrategyArg;
use crate::config::{self, Config, LoadConfigPathOptions};
use crate::docker;
use crate::output::{self, LogLevel, Persistence};

const HEALTHY_POLL_INTERVAL_SECS: u64 = 5;

#[expect(
    clippy::infinite_loop,
    reason = "daemon supervisor is intended to run until killed"
)]
pub(crate) fn run(project_root: &Path) -> Result<()> {
    bootstrap_project(project_root)?;

    let mut consecutive_failures = 0_u32;
    loop {
        match recover_if_needed(project_root) {
            Ok(unhealthy) => {
                consecutive_failures = 0;
                if unhealthy.is_empty() {
                    thread::sleep(Duration::from_secs(HEALTHY_POLL_INTERVAL_SECS));
                } else {
                    output::event(
                        "daemon",
                        LogLevel::Success,
                        &format!(
                            "Recovered service(s) for {}: {}",
                            project_root.display(),
                            unhealthy.join(", ")
                        ),
                        Persistence::Persistent,
                    );
                    thread::sleep(Duration::from_secs(HEALTHY_POLL_INTERVAL_SECS));
                }
            }
            Err(error) => {
                consecutive_failures = consecutive_failures.saturating_add(1);
                let backoff = recovery_backoff_secs(consecutive_failures);
                output::event(
                    "daemon",
                    LogLevel::Error,
                    &format!(
                        "Daemon recovery failed for {}: {}. Retrying in {}s",
                        project_root.display(),
                        error,
                        backoff
                    ),
                    Persistence::Persistent,
                );
                thread::sleep(Duration::from_secs(backoff));
            }
        }
    }
}

fn bootstrap_project(project_root: &Path) -> Result<()> {
    let mut config = load_project_config(project_root)?;
    crate::cli::handlers::handle_start(
        &mut config,
        crate::cli::handlers::HandleStartOptions {
            service: None,
            kind: None,
            profile: None,
            wait: false,
            no_wait: true,
            wait_timeout: None,
            pull_policy: docker::PullPolicy::Missing,
            force_recreate: false,
            open_after_start: false,
            health_path: None,
            include_project_deps: true,
            parallel: crate::cli::args::default_parallelism(),
            quiet: false,
            no_color: false,
            dry_run: false,
            repro: false,
            runtime_env: None,
            config_path: None,
            project_root: Some(project_root),
        },
    )
}

fn recover_if_needed(project_root: &Path) -> Result<Vec<String>> {
    let mut config = load_project_config(project_root)?;
    let unhealthy = non_running_services(&config);
    if unhealthy.is_empty() {
        return Ok(unhealthy);
    }

    crate::cli::handlers::handle_up(
        &mut config,
        crate::cli::handlers::HandleUpOptions {
            service: None,
            kind: None,
            profile: None,
            wait: false,
            no_wait: true,
            wait_timeout: None,
            pull_policy: docker::PullPolicy::Missing,
            force_recreate: false,
            publish_all: false,
            no_publish_all: false,
            port_strategy: PortStrategyArg::Random,
            port_seed: None,
            env_output: false,
            include_project_deps: true,
            seed: false,
            parallel: crate::cli::args::default_parallelism(),
            quiet: false,
            no_color: false,
            dry_run: false,
            repro: false,
            runtime_env: None,
            config_path: None,
            project_root: Some(project_root),
        },
    )?;

    Ok(unhealthy)
}

fn load_project_config(project_root: &Path) -> Result<Config> {
    config::load_config_with(LoadConfigPathOptions::new(None, Some(project_root)))
}

pub(crate) fn non_running_services(config: &Config) -> Vec<String> {
    config
        .service
        .iter()
        .filter(|service| {
            service
                .container_name()
                .ok()
                .and_then(|name| docker::inspect_status(&name))
                .as_deref()
                != Some("running")
        })
        .map(|service| service.name.clone())
        .collect()
}

pub(crate) fn recovery_backoff_secs(consecutive_failures: u32) -> u64 {
    let shifts = consecutive_failures.saturating_sub(1).min(5);
    (1_u64 << shifts).min(60)
}

#[cfg(test)]
mod tests {
    use super::{non_running_services, recovery_backoff_secs};
    use crate::config::{Config, Driver, Kind, ProjectType, ServiceConfig};
    use crate::docker;
    use std::fs;
    use std::io::Write;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn sample_config() -> Config {
        Config {
            schema_version: 1,
            project_type: ProjectType::Project,
            container_prefix: None,
            domain_strategy: None,
            service: vec![service("db"), service("cache"), service("worker")],
            swarm: Vec::new(),
        }
    }

    fn service(name: &str) -> ServiceConfig {
        ServiceConfig {
            name: name.to_owned(),
            kind: Kind::Database,
            driver: Driver::Mysql,
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
            container_name: Some(name.to_owned()),
            resolved_container_name: Some(name.to_owned()),
            runtime: Default::default(),
        }
    }

    fn with_fake_docker<F, T>(script: &str, test: F) -> T
    where
        F: FnOnce() -> T,
    {
        let bin_dir = crate::test_support::temp_root().join(format!(
            "berth-daemon-supervisor-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        // A clock tick is coarse: two parallel tests could share a name and
        // remove each other's fake binary.
        let bin_dir = docker::unique_test_path(bin_dir);
        fs::create_dir_all(&bin_dir).expect("fake docker dir");
        let binary = bin_dir.join("docker");
        let mut file = fs::File::create(&binary).expect("fake docker binary");
        writeln!(file, "#!/bin/sh\n{}", script).expect("script");
        drop(file);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&binary)
                .expect("binary metadata")
                .permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&binary, perms).expect("chmod");
        }

        let result = docker::with_dry_run_state(false, || {
            docker::with_docker_command(&binary.to_string_lossy(), test)
        });
        fs::remove_dir_all(&bin_dir).ok();
        result
    }

    #[test]
    fn non_running_services_detects_missing_and_exited_containers() {
        let config = sample_config();
        with_fake_docker(
            "case \"$3\" in\n\
db) printf 'running'; exit 0;;\n\
cache) printf 'exited'; exit 0;;\n\
worker) exit 1;;\n\
*) exit 1;;\n\
esac",
            || {
                assert_eq!(
                    non_running_services(&config),
                    vec!["cache".to_owned(), "worker".to_owned()]
                );
            },
        );
    }

    #[test]
    fn recovery_backoff_secs_doubles_and_caps() {
        assert_eq!(recovery_backoff_secs(1), 1);
        assert_eq!(recovery_backoff_secs(2), 2);
        assert_eq!(recovery_backoff_secs(3), 4);
        assert_eq!(recovery_backoff_secs(4), 8);
        assert_eq!(recovery_backoff_secs(5), 16);
        assert_eq!(recovery_backoff_secs(6), 32);
        assert_eq!(recovery_backoff_secs(7), 32);
    }
}
