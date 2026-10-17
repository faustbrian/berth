//! docker ops pause module.
//!
//! Contains docker pause operation used by Berth command workflows.

use anyhow::Result;

use crate::config::ServiceConfig;

use super::common::run_simple_container_command;

pub(super) fn pause(service: &ServiceConfig) -> Result<()> {
    run_simple_container_command(
        service,
        "pause",
        &crate::docker::runtime_command_error_context("pause"),
    )
}
