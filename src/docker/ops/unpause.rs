//! docker ops unpause module.
//!
//! Contains docker unpause operation used by Berth command workflows.

use anyhow::Result;

use crate::config::ServiceConfig;

use super::common::run_simple_container_command;

pub(super) fn unpause(service: &ServiceConfig) -> Result<()> {
    run_simple_container_command(
        service,
        "unpause",
        &crate::docker::runtime_command_error_context("unpause"),
    )
}
