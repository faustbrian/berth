//! A container's state as a readiness wait needs it: a wait ends only on
//! what the engine said, never on a failed `inspect` call.

use super::command::docker_inspect_format;

/// What `inspect` established about a container.
#[derive(Debug, Eq, PartialEq)]
pub(crate) enum ContainerState {
    Running,
    /// Exists but does not run (`exited`, `restarting`, `dead`, ...).
    NotRunning(String),
    /// The engine says no such container exists.
    Missing,
    /// `inspect` itself failed (an overloaded or restarting engine); the
    /// next attempt asks again.
    Unknown,
}

/// The container's state; `Running` in a dry run, which starts nothing.
#[must_use]
pub(crate) fn container_state(container_name: &str) -> ContainerState {
    if super::is_dry_run() {
        return ContainerState::Running;
    }
    let Some(output) = docker_inspect_format(container_name, "{{.State.Status}}") else {
        return ContainerState::Unknown;
    };
    if output.status.success() {
        let status = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        return if status == "running" {
            ContainerState::Running
        } else {
            ContainerState::NotRunning(status)
        };
    }
    if crate::docker::reports_missing_container(&String::from_utf8_lossy(&output.stderr)) {
        ContainerState::Missing
    } else {
        ContainerState::Unknown
    }
}

#[cfg(test)]
mod tests {
    use super::{ContainerState, container_state};

    fn with_docker<T>(script: &str, test: impl FnOnce() -> T) -> T {
        let dir = crate::docker::unique_test_path(
            crate::test_support::temp_root().join("container-state"),
        );
        std::fs::create_dir_all(&dir).expect("dir");
        let docker = dir.join("docker");
        std::fs::write(&docker, format!("#!/bin/sh\n{script}\n")).expect("fake docker");
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&docker, std::fs::Permissions::from_mode(0o755))
                .expect("chmod");
        }
        crate::docker::with_dry_run_state(false, || {
            crate::docker::with_docker_command(&docker.to_string_lossy(), test)
        })
    }

    #[test]
    fn only_the_engines_no_such_answer_means_missing() {
        assert_eq!(
            with_docker("echo running", || container_state("app")),
            ContainerState::Running
        );
        assert_eq!(
            with_docker("echo exited", || container_state("app")),
            ContainerState::NotRunning("exited".to_owned())
        );
        assert_eq!(
            with_docker("echo 'Error: No such object: app' >&2; exit 1", || {
                container_state("app")
            }),
            ContainerState::Missing
        );
        assert_eq!(
            with_docker(
                "echo 'error during connect: Get \"http://docker\": EOF' >&2; exit 1",
                || container_state("app")
            ),
            ContainerState::Unknown,
            "an engine that failed to answer says nothing about the container"
        );
        assert_eq!(
            with_docker(
                "echo 'failed to connect to the docker API at unix:///x/docker.sock: dial unix /x/docker.sock: connect: no such file or directory' >&2; exit 1",
                || container_state("app")
            ),
            ContainerState::Unknown,
            "a stopped engine's missing socket is no missing container"
        );
        assert_eq!(
            with_docker("echo 'Error: no such container app' >&2; exit 125", || {
                container_state("app")
            }),
            ContainerState::Missing
        );
    }
}
