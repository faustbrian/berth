//! docker manage module.
//!
//! Contains docker manage logic used by Berth command workflows.

use anyhow::Result;

use crate::config::ServiceConfig;

mod container_ops;
mod image_ops;

/// Downs down as part of the docker manage workflow.
///
/// A shared-backed service only leaves the instance network: the shared
/// server keeps serving other instances.
pub fn down(service: &ServiceConfig, timeout: u64) -> Result<()> {
    if service.runtime.shared.is_some() {
        return crate::shared::detach(service);
    }
    container_ops::down(service, timeout)
}

/// Stops stop as part of the docker manage workflow.
pub fn stop(service: &ServiceConfig, timeout: u64) -> Result<()> {
    if service.runtime.shared.is_some() {
        return crate::shared::detach(service);
    }
    container_ops::stop(service, timeout)
}

/// Rms rm as part of the docker manage workflow.
pub fn rm(service: &ServiceConfig, force: bool) -> Result<()> {
    if service.runtime.shared.is_some() {
        return crate::shared::detach(service);
    }
    container_ops::rm(service, force)
}

pub fn recreate(service: &ServiceConfig) -> Result<()> {
    if service.runtime.shared.is_some() {
        return crate::shared::up(service);
    }
    container_ops::recreate(service)
}

/// Pulls pull as part of the docker manage workflow.
pub fn pull(service: &ServiceConfig) -> Result<()> {
    image_ops::pull(service)
}

/// Restarts restart as part of the docker manage workflow.
pub fn restart(service: &ServiceConfig) -> Result<()> {
    if service.runtime.shared.is_some() {
        return crate::shared::up(service);
    }
    container_ops::restart(service)
}

#[cfg(test)]
mod tests {
    use crate::config::{Driver, Kind, ServiceConfig};
    use crate::docker::{self, with_docker_command};
    use std::fs;
    use std::io::Write;
    use std::time::SystemTime;
    use std::time::UNIX_EPOCH;

    fn service(name: &str, kind: Kind, driver: Driver) -> ServiceConfig {
        ServiceConfig {
            name: name.to_owned(),
            kind,
            driver,
            image: "postgres:16".to_owned(),
            host: "127.0.0.1".to_owned(),
            port: 5432,
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

    fn with_fake_docker<F, T>(script: &str, test: F) -> T
    where
        F: FnOnce() -> T,
    {
        let bin_dir = crate::test_support::temp_root().join(format!(
            "berth-manage-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
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
            with_docker_command(&binary.to_string_lossy(), test)
        });
        fs::remove_dir_all(&bin_dir).ok();
        result
    }

    #[test]
    fn down_runs_docker_commands_in_non_dry_run() {
        let service = service("db", Kind::Database, Driver::Postgres);
        with_fake_docker("exit 0", || {
            super::down(&service, 30).expect("down");
        });
    }

    #[test]
    fn stop_runs_docker_command() {
        let service = service("db", Kind::Database, Driver::Postgres);
        with_fake_docker("exit 0", || {
            super::stop(&service, 30).expect("stop");
        });
    }

    #[test]
    fn rm_runs_docker_commands_in_non_force_mode() {
        let service = service("db", Kind::Database, Driver::Postgres);
        with_fake_docker(
            "case \"$1\" in inspect|rm) exit 0;; *) exit 0;; esac",
            || {
                super::rm(&service, false).expect("rm");
            },
        );
    }

    #[test]
    fn recreate_uses_dry_run_path_when_dry_run_is_enabled() {
        let service = service("db", Kind::Database, Driver::Postgres);
        docker::with_dry_run_lock(|| {
            super::recreate(&service).expect("recreate");
        });
    }

    #[test]
    fn pull_uses_dry_run_or_docker() {
        let service = service("db", Kind::Database, Driver::Postgres);
        docker::with_dry_run_lock(|| {
            super::pull(&service).expect("pull dry-run");
        });
    }

    #[test]
    fn restart_checks_status_and_runs_container_restart() {
        let service = service("db", Kind::Database, Driver::Postgres);
        with_fake_docker(
            "case \"$1\" in\ninspect) printf \"running\";; *) ;; esac\nexit 0",
            || {
                super::restart(&service).expect("restart");
            },
        );
    }

    #[test]
    fn down_runs_dry_run_commands() {
        let service = service("db", Kind::Database, Driver::Postgres);
        docker::with_dry_run_lock(|| {
            super::down(&service, 30).expect("dry run down");
        });
    }
}
