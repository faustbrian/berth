//! docker manage container ops restart module.
//!
//! Contains docker manage container ops restart logic used by Berth command workflows.

use anyhow::Result;

use crate::config::ServiceConfig;
use crate::output::{self, LogLevel, Persistence};

use super::super::super::{is_dry_run, print_docker_command};
use super::docker_cmd::docker_output;

/// Restarts restart as part of the docker manage container ops restart workflow.
pub(super) fn restart(service: &ServiceConfig) -> Result<()> {
    let container_name = service.container_name()?;

    if is_dry_run() {
        print_docker_command(&["restart".to_owned(), container_name]);
        return Ok(());
    }

    match crate::docker::container_state(&container_name) {
        crate::docker::ContainerState::Missing => anyhow::bail!(
            "service '{}' has no container {container_name} to restart; start it with {}",
            service.name,
            crate::followup::for_service(service, &format!("up --service {}", service.name))
        ),
        crate::docker::ContainerState::Unknown => anyhow::bail!(
            "could not inspect {container_name}; is the container engine running? {} checks it",
            crate::followup::for_service(service, "doctor")
        ),
        crate::docker::ContainerState::Running | crate::docker::ContainerState::NotRunning(_) => {}
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
