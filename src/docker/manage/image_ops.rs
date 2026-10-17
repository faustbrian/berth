//! docker manage image ops module.
//!
//! Contains docker manage image ops logic used by Berth command workflows.

use anyhow::Result;

use crate::config::ServiceConfig;
use crate::docker::docker_pull;
use crate::output::{self, LogLevel, Persistence};

use super::super::{is_dry_run, print_docker_command};

/// Pulls pull as part of the docker manage image ops workflow.
pub(super) fn pull(service: &ServiceConfig) -> Result<()> {
    output::event(
        &service.name,
        LogLevel::Info,
        &format!("Pulling image {}", service.image),
        Persistence::Persistent,
    );

    if is_dry_run() {
        print_docker_command(&["pull".to_owned(), service.image.clone()]);
        return Ok(());
    }

    docker_pull(
        &service.image,
        &super::super::runtime_command_error_context("pull"),
        &format!("Failed to pull image {}", service.image),
    )?;

    output::event(
        &service.name,
        LogLevel::Success,
        &format!("Pulled image {}", service.image),
        Persistence::Persistent,
    );
    Ok(())
}
