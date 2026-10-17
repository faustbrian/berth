//! docker inspect command module.
//!
//! Contains docker inspect command logic used by Berth command workflows.

use std::process::Output;

pub(super) fn docker_inspect_format(container_name: &str, format: &str) -> Option<Output> {
    docker_output(&["inspect", &format!("--format={format}"), container_name])
}

pub(super) fn docker_inspect(container_name: &str) -> Option<Output> {
    docker_output(&["inspect", container_name])
}

fn docker_output(args: &[&str]) -> Option<Output> {
    crate::docker::run_docker_output(
        args,
        &crate::docker::runtime_command_error_context("inspect"),
    )
    .ok()
}
