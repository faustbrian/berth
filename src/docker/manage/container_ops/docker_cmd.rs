//! Shared docker container-op command helpers.

use anyhow::Result;
use std::process::Output;

/// Runs `docker` with args and captures output.
pub(super) fn docker_output(args: &[&str], error_context: &str) -> Result<Output> {
    crate::docker::run_docker_output(args, error_context)
}

/// Best-effort docker command used in cleanup paths.
pub(super) fn try_docker_output(args: &[&str]) {
    drop(crate::docker::run_docker_output(
        args,
        &crate::docker::runtime_command_error_context("cleanup"),
    ));
}

/// Whether a failed container command failed only because the container is
/// gone (Docker: "No such container", Podman: "no such container"; one
/// started with `--rm` removes itself once stopped). Any other failure is
/// the engine's and must not pass as done.
pub(super) fn reports_missing(output: &Output) -> bool {
    crate::docker::reports_missing_container(&String::from_utf8_lossy(&output.stderr))
}

/// Fails with the engine's own words for a failure other than a missing
/// container.
pub(super) fn engine_failure(action: &str, container: &str, output: &Output) -> anyhow::Error {
    anyhow::anyhow!(
        "failed to {action} container {container}: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    )
}

#[cfg(test)]
mod tests {
    use super::reports_missing;
    use std::os::unix::process::ExitStatusExt;
    use std::process::{ExitStatus, Output};

    fn failed(stderr: &str) -> Output {
        Output {
            status: ExitStatus::from_raw(1 << 8),
            stdout: Vec::new(),
            stderr: stderr.as_bytes().to_vec(),
        }
    }

    #[test]
    fn only_a_missing_container_counts_as_missing() {
        assert!(reports_missing(&failed(
            "Error response from daemon: No such container: x"
        )));
        assert!(reports_missing(&failed("Error: no such container x")));
        assert!(!reports_missing(&failed(
            "permission denied while trying to connect to the Docker daemon socket"
        )));
        for unreachable in [
            "failed to connect to the docker API at unix:///x/docker.sock: dial unix /x/docker.sock: connect: no such file or directory",
            "error during connect: dial tcp: lookup dockerhost: no such host",
        ] {
            assert!(!reports_missing(&failed(unreachable)), "{unreachable}");
        }
    }
}
