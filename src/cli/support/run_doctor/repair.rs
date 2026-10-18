//! cli support run doctor repair module.
//!
//! Contains cli support run doctor repair logic used by Berth command workflows.

use crate::config;
use crate::docker;

use super::report;

const PROBE_TIMEOUT_SECS: u64 = 3;
const PROBE_INTERVAL_SECS: u64 = 1;
const RESTART_REPAIR_TIMEOUT_SECS: u64 = 5;
const RECREATE_REPAIR_TIMEOUT_SECS: u64 = 30;
const REPAIR_INTERVAL_SECS: u64 = 2;

/// Repairs unhealthy runtime services for doctor fix workflow.
pub(super) fn repair_unhealthy_services(config: &config::Config) -> bool {
    let mut has_error = false;

    for service in &config.service {
        if service_is_healthy(service, PROBE_TIMEOUT_SECS, PROBE_INTERVAL_SECS, Some(1)) {
            continue;
        }

        report::info(&format!(
            "Service '{}' appears unhealthy; attempting restart repair",
            service.name
        ));
        if restart_then_wait(service) {
            report::success(&format!(
                "Service '{}' recovered after restart",
                service.name
            ));
            continue;
        }

        report::info(&format!(
            "Service '{}' still unhealthy; attempting recreate repair",
            service.name
        ));
        if recreate_then_wait(service) {
            report::success(&format!(
                "Service '{}' recovered after recreate",
                service.name
            ));
            continue;
        }

        report::error(&format!(
            "Service '{}' failed automatic repair after restart and recreate",
            service.name
        ));
        has_error = true;
    }

    has_error
}

fn service_is_healthy(
    service: &config::ServiceConfig,
    timeout_secs: u64,
    interval_secs: u64,
    retries: Option<u32>,
) -> bool {
    docker::wait_until_healthy(service, timeout_secs, interval_secs, retries).is_ok()
}

fn restart_then_wait(service: &config::ServiceConfig) -> bool {
    docker::restart(service).is_ok()
        && service_is_healthy(
            service,
            RESTART_REPAIR_TIMEOUT_SECS,
            REPAIR_INTERVAL_SECS,
            None,
        )
}

fn recreate_then_wait(service: &config::ServiceConfig) -> bool {
    docker::recreate(service).is_ok()
        && docker::up(service, docker::PullPolicy::Missing, false).is_ok()
        && service_is_healthy(
            service,
            RECREATE_REPAIR_TIMEOUT_SECS,
            REPAIR_INTERVAL_SECS,
            None,
        )
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io::Write;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    use crate::config::{Config, Driver, Kind, ServiceConfig};
    use crate::docker::with_docker_command;

    use super::repair_unhealthy_services;

    #[test]
    fn repair_unhealthy_services_restarts_unhealthy_service() {
        let fixture = TempDoctorRepairFixture::new(true);
        let config = config_with_service();

        let has_error = fixture.with_fake_docker(|| repair_unhealthy_services(&config));
        assert!(!has_error);

        let log = fs::read_to_string(&fixture.docker_log_path).expect("read docker log");
        assert!(
            log.contains("restart db"),
            "expected repair flow to restart unhealthy service"
        );
    }

    #[test]
    fn repair_unhealthy_services_recreates_when_restart_does_not_recover() {
        let fixture = TempDoctorRepairFixture::new(false);
        let config = config_with_service();

        let has_error = fixture.with_fake_docker(|| repair_unhealthy_services(&config));
        assert!(!has_error);

        let log = fs::read_to_string(&fixture.docker_log_path).expect("read docker log");
        assert!(
            log.contains("restart db"),
            "expected repair flow to attempt restart first"
        );
        assert!(
            log.contains("rm -v db"),
            "expected repair flow to recreate after failed restart"
        );
        assert!(
            log.contains("run "),
            "expected repair flow to run recreated container"
        );
    }

    struct TempDoctorRepairFixture {
        docker_bin_path: PathBuf,
        docker_log_path: PathBuf,
        recreated_marker_path: PathBuf,
        restarted_marker_path: PathBuf,
        removed_marker_path: PathBuf,
        root: PathBuf,
    }

    impl TempDoctorRepairFixture {
        fn new(restart_recovers: bool) -> Self {
            let root = crate::test_support::temp_root().join(format!(
                "berth-doctor-repair-all-{}",
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .expect("time")
                    .as_nanos()
            ));
            fs::create_dir_all(&root).expect("create fixture dir");

            let docker_bin_path = root.join("docker");
            let docker_log_path = root.join("docker.log");
            let recreated_marker_path = root.join("recreated");
            let restarted_marker_path = root.join("restarted");
            let removed_marker_path = root.join("removed");
            fs::write(&docker_log_path, "").expect("init docker log");
            fs::write(&removed_marker_path, "").expect("init removed marker");
            fs::remove_file(&removed_marker_path).ok();

            write_executable(
                &docker_bin_path,
                &format!(
                    "#!/bin/sh\n\
                    echo \"$@\" >> \"{}\"\n\
                    if [ \"$1\" = \"inspect\" ] && [ \"$2\" = \"--format={{{{.State.Status}}}}\" ]; then\n\
                      if [ -f \"{}\" ] && [ ! -f \"{}\" ]; then\n\
                        exit 1\n\
                      fi\n\
                      printf \"running\"\n\
                      exit 0\n\
                    fi\n\
                    if [ \"$1\" = \"restart\" ]; then\n\
                      touch \"{}\"\n\
                      exit 0\n\
                    fi\n\
                    if [ \"$1\" = \"stop\" ]; then\n\
                      exit 0\n\
                    fi\n\
                    if [ \"$1\" = \"rm\" ]; then\n\
                      touch \"{}\"\n\
                      exit 0\n\
                    fi\n\
                    if [ \"$1\" = \"image\" ] && [ \"$2\" = \"inspect\" ]; then\n\
                      exit 0\n\
                    fi\n\
                    if [ \"$1\" = \"run\" ]; then\n\
                      touch \"{}\"\n\
                      rm -f \"{}\"\n\
                      exit 0\n\
                    fi\n\
                    if [ \"$1\" = \"exec\" ]; then\n\
                      if [ -f \"{}\" ]; then\n\
                        exit 0\n\
                      fi\n\
                      if [ {} -eq 1 ] && [ -f \"{}\" ]; then\n\
                        exit 0\n\
                      fi\n\
                      exit 1\n\
                    fi\n\
                    exit 0\n",
                    docker_log_path.display(),
                    removed_marker_path.display(),
                    recreated_marker_path.display(),
                    restarted_marker_path.display(),
                    removed_marker_path.display(),
                    recreated_marker_path.display(),
                    removed_marker_path.display(),
                    recreated_marker_path.display(),
                    if restart_recovers { 1 } else { 0 },
                    restarted_marker_path.display(),
                ),
            );

            Self {
                docker_bin_path,
                docker_log_path,
                recreated_marker_path,
                restarted_marker_path,
                removed_marker_path,
                root,
            }
        }

        fn with_fake_docker<F, T>(&self, test: F) -> T
        where
            F: FnOnce() -> T,
        {
            let command = self.docker_bin_path.to_string_lossy().to_string();
            crate::docker::with_dry_run_state(false, || with_docker_command(&command, test))
        }
    }

    impl Drop for TempDoctorRepairFixture {
        fn drop(&mut self) {
            fs::remove_file(&self.recreated_marker_path).ok();
            fs::remove_file(&self.restarted_marker_path).ok();
            fs::remove_file(&self.removed_marker_path).ok();
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

    fn config_with_service() -> Config {
        Config {
            schema_version: 1,
            project_type: crate::config::ProjectType::Project,
            container_prefix: None,
            domain_strategy: None,
            service: vec![ServiceConfig {
                name: "db".to_owned(),
                kind: Kind::Database,
                driver: Driver::Postgres,
                image: "postgres:16".to_owned(),
                host: "127.0.0.1".to_owned(),
                port: 5432,
                database: Some("app".to_owned()),
                username: Some("app".to_owned()),
                password: Some("secret".to_owned()),
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
                container_name: Some("db".to_owned()),
                resolved_container_name: Some("db".to_owned()),
                runtime: Default::default(),
            }],
            swarm: Vec::new(),
        }
    }
}
