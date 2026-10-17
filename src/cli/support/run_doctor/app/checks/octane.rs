//! doctor app octane runtime checks.

use crate::cli::support::run_doctor::report;
use crate::{config, serve};

/// Checks octane runtime and reports actionable failures.
pub(in crate::cli::support::run_doctor::app) fn check_octane_runtime(
    target: &config::ServiceConfig,
    allow_stopped_runtime_checks: bool,
) -> bool {
    match serve::runtime_cmdline(target) {
        Ok(Some(cmdline)) if cmdline.contains("octane:frankenphp") => {
            report::success(&format!(
                "App service '{}' running with octane",
                target.name
            ));
            false
        }
        Ok(Some(cmdline)) => {
            report::error(&format!(
                "App service '{}' not running octane (pid1: {})",
                target.name, cmdline
            ));
            true
        }
        Ok(None) => {
            if allow_stopped_runtime_checks {
                report::info(&format!(
                    "App service '{}' is not running; octane runtime check deferred",
                    target.name
                ));
                return false;
            }
            report::error(&format!(
                "App service '{}' is not running for octane check",
                target.name
            ));
            true
        }
        Err(err) => {
            report::error(&format!(
                "App service '{}' octane check failed: {}",
                target.name, err
            ));
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{config::Driver, config::Kind, config::ServiceConfig, docker};

    use super::check_octane_runtime;

    fn service() -> ServiceConfig {
        ServiceConfig {
            name: "app".to_owned(),
            kind: Kind::App,
            driver: Driver::Frankenphp,
            image: "dunglas/frankenphp:php8.5".to_owned(),
            host: "127.0.0.1".to_owned(),
            port: 3300,
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
        }
    }

    fn with_fake_docker<F, T>(inspect_status: &str, exec_cmdline: Option<&str>, test: F) -> T
    where
        F: FnOnce() -> T,
    {
        let bin_dir = std::env::temp_dir().join(format!(
            "berth-doctor-octane-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        std::fs::create_dir_all(&bin_dir).expect("create temp dir");

        let docker_bin = bin_dir.join("docker");
        let mut file = std::fs::File::create(&docker_bin).expect("create fake docker");
        use std::io::Write;
        match exec_cmdline {
            Some(cmdline) => {
                writeln!(
                    file,
                    "#!/bin/sh\nif [ \"$1\" = \"inspect\" ]; then\n  printf '{}';\n  exit 0;\nfi\nif [ \"$1\" = \"exec\" ]; then\n  echo '{}';\n  exit 0;\nfi\nexit 0\n",
                    inspect_status,
                    cmdline
                )
                .expect("write script");
            }
            None => {
                writeln!(
                    file,
                    "#!/bin/sh\nif [ \"$1\" = \"inspect\" ]; then\n  printf '{}';\n  exit 0;\nfi\nexit 0\n",
                    inspect_status
                )
                .expect("write script");
            }
        }
        drop(file);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&docker_bin)
                .expect("metadata")
                .permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&docker_bin, perms).expect("chmod");
        }

        let command = docker_bin.to_string_lossy().to_string();
        let result =
            docker::with_dry_run_state(false, || docker::with_docker_command(&command, || test()));
        std::fs::remove_dir_all(&bin_dir).ok();
        result
    }

    #[test]
    fn check_octane_runtime_returns_false_for_octane_commandline() {
        with_fake_docker("running", Some("php artisan octane:frankenphp"), || {
            assert!(!check_octane_runtime(&service(), false));
        });
    }

    #[test]
    fn check_octane_runtime_reports_error_when_octane_is_missing() {
        with_fake_docker("running", Some("php artisan serve"), || {
            assert!(check_octane_runtime(&service(), false));
        });
    }

    #[test]
    fn check_octane_runtime_defers_when_not_running_and_allowed() {
        with_fake_docker("exited", None, || {
            assert!(!check_octane_runtime(&service(), true));
        });
    }

    #[test]
    fn check_octane_runtime_flags_stopped_service_as_error_when_forbidden() {
        with_fake_docker("exited", None, || {
            assert!(check_octane_runtime(&service(), false));
        });
    }

    #[test]
    fn check_octane_runtime_flags_execution_error() {
        let target = service();
        let result = docker::with_docker_command("/definitely/missing/docker", || {
            check_octane_runtime(&target, false)
        });
        assert!(result);
    }
}
