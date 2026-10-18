//! cli support run doctor app module.
//!
//! Contains cli support run doctor app logic used by Berth command workflows.

use crate::config;

use super::super::app_services::app_services;
use checks::{
    check_domain_resolution, check_http_reachability, check_octane_runtime, check_php_extensions,
    repair_unhealthy_http_runtime,
};

mod checks;

/// Checks app services and reports actionable failures.
pub(super) fn check_app_services(
    config: &config::Config,
    fix: bool,
    allow_stopped_runtime_checks: bool,
) -> bool {
    let mut has_error = false;

    for target in app_services(config) {
        has_error |= check_domain_resolution(target, fix);
        has_error |= check_php_extensions(target);
        has_error |= repair_unhealthy_http_runtime(target, fix);

        if target.octane {
            has_error |= check_octane_runtime(target, allow_stopped_runtime_checks);
        }
    }

    has_error
}

/// Checks app URL and health endpoint reachability.
pub(super) fn check_reachability(config: &config::Config) -> bool {
    let mut has_error = false;

    for target in app_services(config) {
        has_error |= check_http_reachability(target);
    }

    has_error
}

#[cfg(test)]
mod tests {
    use crate::cli::support::with_curl_command;
    use crate::config::{Config, Driver, Kind, ServiceConfig};
    use crate::docker::with_docker_command;
    use std::fs;
    use std::io::Write;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::check_app_services;

    #[test]
    fn check_app_services_fix_restarts_unhealthy_http_app() {
        let fixture = TempDoctorRepairFixture::new("503", "200");
        let config = Config {
            schema_version: 1,
            project_type: crate::config::ProjectType::Project,
            container_prefix: None,
            domain_strategy: None,
            service: vec![app_service()],
            swarm: Vec::new(),
        };

        fixture.with_fake_commands(|| {
            let has_error = check_app_services(&config, true, false);
            assert!(!has_error);
        });

        let docker_log = fs::read_to_string(&fixture.docker_log_path).expect("read docker log");
        assert!(
            docker_log.contains("restart app"),
            "expected doctor --fix to restart unhealthy app service"
        );
    }

    struct TempDoctorRepairFixture {
        docker_bin_path: PathBuf,
        curl_bin_path: PathBuf,
        docker_log_path: PathBuf,
        root: PathBuf,
    }

    impl TempDoctorRepairFixture {
        fn new(unhealthy_status: &str, healthy_status: &str) -> Self {
            let root = crate::test_support::temp_root().join(format!(
                "berth-doctor-repair-{}",
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .expect("time")
                    .as_nanos()
            ));
            fs::create_dir_all(&root).expect("create temp fixture");
            let docker_log_path = root.join("docker.log");
            fs::write(&docker_log_path, "").expect("init docker log");
            let repaired_marker = root.join("repaired");
            let docker_bin_path = root.join("docker");
            let curl_bin_path = root.join("curl");

            write_executable(
                &docker_bin_path,
                &format!(
                    "#!/bin/sh\n\
                    echo \"$@\" >> \"{}\"\n\
                    if [ \"$1\" = \"inspect\" ]; then\n\
                      printf \"running\"\n\
                      exit 0\n\
                    fi\n\
                    if [ \"$1\" = \"restart\" ]; then\n\
                      touch \"{}\"\n\
                      exit 0\n\
                    fi\n\
                    exit 0\n",
                    docker_log_path.display(),
                    repaired_marker.display(),
                ),
            );
            write_executable(
                &curl_bin_path,
                &format!(
                    "#!/bin/sh\n\
                    if [ -f \"{}\" ]; then\n\
                      printf \"{}\"\n\
                    else\n\
                      printf \"{}\"\n\
                    fi\n",
                    repaired_marker.display(),
                    healthy_status,
                    unhealthy_status,
                ),
            );

            Self {
                docker_bin_path,
                curl_bin_path,
                docker_log_path,
                root,
            }
        }

        fn with_fake_commands<F, T>(&self, test: F) -> T
        where
            F: FnOnce() -> T,
        {
            let docker_bin = self.docker_bin_path.to_string_lossy().to_string();
            let curl_bin = self.curl_bin_path.to_string_lossy().to_string();
            crate::docker::with_dry_run_state(false, || {
                with_docker_command(&docker_bin, || with_curl_command(&curl_bin, test))
            })
        }
    }

    impl Drop for TempDoctorRepairFixture {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.root).ok();
        }
    }

    fn write_executable(path: &Path, body: &str) {
        let mut file = fs::File::create(path).expect("create script");
        file.write_all(body.as_bytes()).expect("write script");
        drop(file);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(path).expect("metadata").permissions();
            perms.set_mode(0o755);
            fs::set_permissions(path, perms).expect("chmod");
        }
    }

    fn app_service() -> ServiceConfig {
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
            scheme: Some("https".to_owned()),
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
            runtime: Default::default(),
        }
    }
}
