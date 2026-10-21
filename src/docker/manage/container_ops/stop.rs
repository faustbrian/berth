//! docker manage container ops stop module.
//!
//! Contains docker manage container ops stop logic used by Berth command workflows.

use anyhow::Result;

use crate::config::ServiceConfig;
use crate::output::{self, LogLevel, Persistence};

use super::super::super::{is_dry_run, print_docker_command};
use super::docker_cmd::{docker_output, engine_failure, reports_missing};

/// Stops stop as part of the docker manage container ops stop workflow.
pub(super) fn stop(service: &ServiceConfig, timeout: u64) -> Result<()> {
    let container_name = service.container_name()?;

    if is_dry_run() {
        print_docker_command(&[
            "stop".to_owned(),
            "--time".to_owned(),
            timeout.to_string(),
            container_name,
        ]);
        return Ok(());
    }

    crate::docker::ensure_own_container(&container_name)?;
    let output = docker_output(
        &["stop", "--time", &timeout.to_string(), &container_name],
        &crate::docker::runtime_command_error_context("stop"),
    )?;

    if output.status.success() {
        output::event(
            &service.name,
            LogLevel::Success,
            &format!("Stopped container {container_name}"),
            Persistence::Persistent,
        );
    } else if reports_missing(&output) {
        output::event(
            &service.name,
            LogLevel::Info,
            &format!("Skipped stopping container {container_name} because it does not exist"),
            Persistence::Persistent,
        );
    } else {
        return Err(engine_failure("stop", &container_name, &output));
    }

    Ok(())
}
