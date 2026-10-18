//! docker manage container ops restart module.
//!
//! Contains docker manage container ops restart logic used by Berth command workflows.

use anyhow::Result;

use crate::config::ServiceConfig;
use crate::output::{self, LogLevel, Persistence};

use super::super::super::{inspect_status, is_dry_run, print_docker_command};
use super::docker_cmd::docker_output;

/// Restarts restart as part of the docker manage container ops restart workflow.
pub(super) fn restart(service: &ServiceConfig) -> Result<()> {
    let container_name = service.container_name()?;

    if is_dry_run() {
        print_docker_command(&["restart".to_owned(), container_name]);
        return Ok(());
    }

    if inspect_status(&container_name).is_none() {
        anyhow::bail!("Container {container_name} does not exist");
    }
    crate::docker::ensure_own_container(&container_name)?;

    let output = docker_output(
        &["restart", &container_name],
        &crate::docker::runtime_command_error_context("restart"),
    )?;
    crate::docker::ensure_docker_output_success(output, "Failed to restart container")?;

    output::event(
        &service.name,
        LogLevel::Success,
        &format!("Restarted container {container_name}"),
        Persistence::Persistent,
    );
    Ok(())
}
