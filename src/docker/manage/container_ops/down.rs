//! docker manage container ops down module.
//!
//! Contains docker manage container ops down logic used by Berth command workflows.

use anyhow::Result;

use crate::config::ServiceConfig;
use crate::output::{self, LogLevel, Persistence};

use super::super::super::{is_dry_run, print_docker_command};
use super::docker_cmd::{docker_output, engine_failure, reports_missing};

/// Downs down as part of the docker manage container ops down workflow.
pub(super) fn down(service: &ServiceConfig, timeout: u64) -> Result<()> {
    let container_name = service.container_name()?;

    if is_dry_run() {
        print_docker_command(&[
            "stop".to_owned(),
            "--time".to_owned(),
            timeout.to_string(),
            container_name.clone(),
        ]);
        print_docker_command(&["rm".to_owned(), container_name]);
        return Ok(());
    }

    crate::docker::ensure_own_container(&container_name)?;
    let stop_output = docker_output(
        &["stop", "--time", &timeout.to_string(), &container_name],
        &crate::docker::runtime_command_error_context("stop"),
    )?;

    if stop_output.status.success() {
        output::event(
            &service.name,
            LogLevel::Success,
            &format!("Stopped container {container_name}"),
            Persistence::Persistent,
        );
    } else if reports_missing(&stop_output) {
        output::event(
            &service.name,
            LogLevel::Info,
            &format!("Skipped stopping container {container_name} because it does not exist"),
            Persistence::Persistent,
        );
    } else {
        return Err(engine_failure("stop", &container_name, &stop_output));
    }

    let rm_output = docker_output(
        &["rm", &container_name],
        &crate::docker::runtime_command_error_context("rm"),
    )?;

    if rm_output.status.success() {
        output::event(
            &service.name,
            LogLevel::Success,
            &format!("Removed container {container_name}"),
            Persistence::Persistent,
        );
    } else if reports_missing(&rm_output) {
        // Gone already: one started with `--rm` removed itself when stopped.
        output::event(
            &service.name,
            LogLevel::Info,
            &format!("Container {container_name} is gone"),
            Persistence::Persistent,
        );
    } else {
        return Err(engine_failure("remove", &container_name, &rm_output));
    }

    Ok(())
}
