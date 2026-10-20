//! Shared low-level docker command runners.

use anyhow::{Context, Result};
use std::process::{Child, ExitStatus, Output, Stdio};

/// Converts owned docker args into borrowed args for command execution.
pub(crate) fn docker_arg_refs(args: &[String]) -> Vec<&str> {
    args.iter().map(String::as_str).collect()
}

/// Runs `docker` with args and captures output.
pub(crate) fn run_docker_output(args: &[&str], context: &str) -> Result<Output> {
    super::ensure_engine_installed()?;
    super::secret_env::docker_process(args)?
        .output()
        .with_context(|| context.to_owned())
}

/// Runs `docker` with owned args and captures output.
pub(crate) fn run_docker_output_owned(args: &[String], context: &str) -> Result<Output> {
    let arg_refs = docker_arg_refs(args);
    run_docker_output(&arg_refs, context)
}

/// Runs `docker` with args and waits for exit status.
pub(crate) fn run_docker_status(args: &[&str], context: &str) -> Result<ExitStatus> {
    super::ensure_engine_installed()?;
    super::secret_env::docker_process(args)?
        .stdout(crate::output::child_stdout())
        .status()
        .with_context(|| context.to_owned())
}

/// Runs `docker` with owned args and waits for exit status.
pub(crate) fn run_docker_status_owned(args: &[String], context: &str) -> Result<ExitStatus> {
    let arg_refs = docker_arg_refs(args);
    run_docker_status(&arg_refs, context)
}

/// Spawns `docker` with stdin/stderr piped.
pub(crate) fn spawn_docker_stdin_stderr_piped(args: &[&str], context: &str) -> Result<Child> {
    super::ensure_engine_installed()?;
    super::secret_env::docker_process(args)?
        .stdin(Stdio::piped())
        .stdout(crate::output::child_stdout())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| context.to_owned())
}

/// Spawns `docker` with stdin, stdout and stderr piped.
pub(crate) fn spawn_docker_stdin_stdout_stderr_piped(
    args: &[&str],
    context: &str,
) -> Result<Child> {
    super::ensure_engine_installed()?;
    super::secret_env::docker_process(args)?
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| context.to_owned())
}

/// Spawns `docker` with stdout/stderr piped.
pub(crate) fn spawn_docker_stdout_stderr_piped(args: &[&str], context: &str) -> Result<Child> {
    super::ensure_engine_installed()?;
    super::secret_env::docker_process(args)?
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| context.to_owned())
}

/// Ensures a docker command output succeeded, mapping stderr on failure.
pub(crate) fn ensure_docker_output_success(output: Output, error_prefix: &str) -> Result<Output> {
    if output.status.success() {
        return Ok(output);
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    anyhow::bail!("{error_prefix}: {stderr}");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::docker;
    use std::fs;
    use std::io::Write;
    use std::time::SystemTime;
    use std::time::UNIX_EPOCH;

    fn with_fake_docker<F, T>(script: &str, test: F) -> T
    where
        F: FnOnce() -> T,
    {
        let bin_dir = crate::test_support::temp_root().join(format!(
            "berth-fake-docker-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("time")
                .as_nanos()
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
    fn docker_arg_refs_reuses_owned_values() {
        let args = vec!["ps".to_owned(), "-q".to_owned()];
        let refs = docker_arg_refs(&args);
        assert_eq!(refs, vec!["ps", "-q"]);
    }

    #[test]
    fn run_docker_output_captures_stdout() {
        with_fake_docker("printf '%s' out && exit 0", || {
            let output = run_docker_output(&["output"], "ok").expect("run docker");
            assert!(output.status.success());
            assert_eq!(String::from_utf8_lossy(&output.stdout), "out");
        });
    }

    #[test]
    fn run_docker_status_reflects_exit_code() {
        with_fake_docker("exit 2", || {
            let status = run_docker_status(&["fail"], "status failure")
                .expect("status command should execute");
            assert!(!status.success());
        });
    }

    #[test]
    fn run_docker_output_owned_delegates_to_owned_refs() {
        let args = vec!["owned".to_owned()];
        with_fake_docker("printf '%s' owned-out && exit 0", || {
            let output = run_docker_output_owned(&args, "owned").expect("run owned");
            assert_eq!(String::from_utf8_lossy(&output.stdout), "owned-out");
        });
    }

    #[test]
    fn run_docker_status_owned_reflects_exit_code() {
        let args = vec!["owned-status".to_owned()];
        with_fake_docker("exit 7", || {
            let status = run_docker_status_owned(&args, "owned status").expect("status");
            assert_eq!(status.code(), Some(7));
        });
    }

    #[test]
    fn spawn_docker_stdout_stderr_piped_returns_child() {
        with_fake_docker(
            "#!/bin/sh\nprintf '%s' stdout\nprintf '%s' stderr 1>&2\nexit 0",
            || {
                let child = spawn_docker_stdout_stderr_piped(&["echo"], "spawn stdout")
                    .expect("spawn command");
                let output = child.wait_with_output().expect("collect output");
                assert!(output.status.success());
                assert_eq!(String::from_utf8_lossy(&output.stdout), "stdout");
                assert_eq!(String::from_utf8_lossy(&output.stderr), "stderr");
            },
        );
    }

    #[test]
    fn spawn_docker_stdin_stderr_piped_returns_child() {
        with_fake_docker("#!/bin/sh\nprintf '%s' stdin-stderr\nexit 0", || {
            let child =
                spawn_docker_stdin_stderr_piped(&["echo"], "spawn stdin").expect("spawn command");
            let output = child.wait_with_output().expect("collect output");
            assert!(output.status.success());
        });
    }

    #[test]
    fn ensure_docker_output_success_maps_failure_with_stderr() {
        with_fake_docker("printf '%s' boom 1>&2; exit 1", || {
            let output =
                run_docker_output(&["fail"], "docker failed").expect("failed docker command");
            let error =
                ensure_docker_output_success(output, "docker failed").expect_err("expected error");
            assert!(error.to_string().contains("docker failed"));
            assert!(error.to_string().contains("boom"));
        });
    }
}
