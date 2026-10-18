//! cli support run doctor docker module.
//!
//! Contains cli support run doctor docker logic used by Berth command workflows.

use super::report;

/// Checks selected container runtime availability and reports failures.
pub(super) fn check_runtime_availability() -> bool {
    crate::docker::runtime_diagnostic_checks()
        .iter()
        .any(run_runtime_check)
}

fn run_runtime_check(check: &crate::docker::RuntimeDiagnosticCheck) -> bool {
    let output = crate::docker::run_docker_output(&[check.arg], "failed to execute runtime check");
    match output {
        Ok(output) if output.status.success() => {
            report::success(check.success_message);
            false
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            report::error(&format!("{}: {stderr}", check.failed_output_prefix));
            true
        }
        Err(err) => {
            report::error(&format!("{}: {err}", check.failed_exec_prefix));
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::check_runtime_availability;
    use std::fs;
    use std::io::Write;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn with_fake_runtime<F, T>(script_template: &str, test: F) -> T
    where
        F: FnOnce(std::path::PathBuf) -> T,
    {
        let root = crate::test_support::temp_root().join(format!(
            "berth-doctor-runtime-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        fs::create_dir_all(&root).expect("create temp root");
        let log_path = root.join("runtime.log");
        fs::write(&log_path, "").expect("init log");

        let script = script_template.replace("__LOG_PATH__", &log_path.to_string_lossy());
        let binary = root.join("docker");
        let mut file = fs::File::create(&binary).expect("create fake runtime");
        writeln!(file, "#!/bin/sh\n{}", script).expect("write script");
        drop(file);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&binary).expect("metadata").permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&binary, perms).expect("chmod");
        }

        let command = binary.to_string_lossy().to_string();
        let result = crate::docker::with_docker_command(&command, || test(log_path.clone()));
        fs::remove_dir_all(&root).ok();
        result
    }

    #[test]
    fn runtime_doctor_checks_run_all_diagnostics_on_success() {
        with_fake_runtime(
            "printf '%s\\n' \"$1\" >> '__LOG_PATH__'; exit 0",
            |log_path| {
                crate::docker::with_container_engine(
                    crate::config::ContainerEngine::Docker,
                    || {
                        let failed = check_runtime_availability();
                        assert!(!failed);
                        let log = fs::read_to_string(log_path).expect("read log");
                        assert!(log.contains("--version"));
                        assert!(log.contains("info"));
                    },
                );
            },
        );
    }

    #[test]
    fn runtime_doctor_checks_report_failure_when_info_fails() {
        with_fake_runtime(
            "printf '%s\\n' \"$1\" >> '__LOG_PATH__'; [ \"$1\" = \"info\" ] && exit 1; exit 0",
            |log_path| {
                crate::docker::with_container_engine(
                    crate::config::ContainerEngine::Podman,
                    || {
                        let failed = check_runtime_availability();
                        assert!(failed);
                        let log = fs::read_to_string(log_path).expect("read log");
                        assert!(log.contains("--version"));
                        assert!(log.contains("info"));
                    },
                );
            },
        );
    }
}
