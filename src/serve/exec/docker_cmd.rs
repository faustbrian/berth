//! Shared docker command execution helpers for serve exec flows.

use anyhow::Result;
use std::process::Output;

use crate::output::{self, LogLevel, Persistence};

pub(super) fn run_or_log_docker(
    target_name: &str,
    args: &[String],
    execution_context: &str,
    failed_message: impl FnOnce(Option<i32>) -> String,
) -> Result<()> {
    if crate::docker::is_dry_run() {
        output::event(
            target_name,
            LogLevel::Info,
            &format!("[dry-run] {}", crate::docker::runtime_command_text(args)),
            Persistence::Transient,
        );
        return Ok(());
    }

    let status = crate::docker::run_docker_status_owned(args, execution_context)?;
    if !status.success() {
        return Err(crate::docker::command_failed_error(
            status,
            failed_message(status.code()),
        ));
    }
    Ok(())
}

pub(super) fn docker_output(args: &[String], execution_context: &str) -> Result<Output> {
    crate::docker::run_docker_output_owned(args, execution_context)
}

#[cfg(test)]
mod tests {
    use super::run_or_log_docker;
    use crate::docker;
    use std::fs;
    use std::io::Write;

    fn with_fake_docker<F, T>(script: &str, test: F) -> T
    where
        F: FnOnce() -> T,
    {
        let bin_dir = crate::test_support::temp_root().join(format!(
            "berth-fake-serve-exec-{}",
            crate::test_support::unique_stamp()
        ));
        // A clock tick is coarse: two parallel tests could share a name and
        // remove each other's fake binary.
        let bin_dir = docker::unique_test_path(bin_dir);
        fs::create_dir_all(&bin_dir).expect("create temp dir");

        let binary = bin_dir.join("docker");
        let mut file = fs::File::create(&binary).expect("create fake docker");
        writeln!(file, "#!/bin/sh\n{}", script).expect("write script");
        drop(file);

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;

            let mut perms = fs::metadata(&binary).expect("metadata").permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&binary, perms).expect("chmod");
        }

        let binary = binary.to_string_lossy().to_string();
        let result = docker::with_docker_command(&binary, test);
        fs::remove_dir_all(&bin_dir).ok();
        result
    }

    #[test]
    fn run_or_log_docker_includes_exit_code_in_failure_message() {
        with_fake_docker("exit 17", || {
            let error = run_or_log_docker(
                "app",
                &["exec".to_owned(), "container".to_owned()],
                "failed to execute artisan command in serve container",
                |exit_code| format!("failed with {:?}", exit_code),
            )
            .expect_err("docker exec should fail");

            assert!(error.to_string().contains("Some(17)"));
        });
    }
}
