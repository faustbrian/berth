//! docker manage container ops rm module.
//!
//! Contains docker manage container ops rm logic used by Berth command workflows.

use anyhow::Result;

use crate::config::ServiceConfig;
use crate::output::{self, LogLevel, Persistence};

use super::super::super::{inspect_status, is_dry_run, print_docker_command};
use super::docker_cmd::{docker_output, try_docker_output};

/// Rms rm as part of the docker manage container ops rm workflow.
pub(super) fn rm(service: &ServiceConfig, force: bool) -> Result<()> {
    let container_name = service.container_name()?;

    if is_dry_run() {
        if force {
            print_docker_command(&["stop".to_owned(), container_name.clone()]);
        }
        print_docker_command(&["rm".to_owned(), container_name]);
        return Ok(());
    }

    crate::docker::ensure_own_container(&container_name)?;
    if force {
        try_docker_output(&["stop", &container_name]);
    } else if let Some(status) = inspect_status(&container_name)
        && status == "running"
    {
        anyhow::bail!(
            "Container '{}' is still running. Stop it first with 'berth stop --service {}' or use --force",
            container_name,
            service.name
        );
    }

    let output = docker_output(
        &["rm", &container_name],
        &crate::docker::runtime_command_error_context("rm"),
    )?;

    if output.status.success() {
        output::event(
            &service.name,
            LogLevel::Success,
            &format!("Removed container {container_name}"),
            Persistence::Persistent,
        );
    } else {
        output::event(
            &service.name,
            LogLevel::Info,
            &format!("Skipped removing container {container_name} because it does not exist"),
            Persistence::Persistent,
        );
    }

    Ok(())
}
