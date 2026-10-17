//! docker ops wait module.
//!
//! Contains docker wait operation used by Berth command workflows.

use anyhow::Result;

use crate::config::ServiceConfig;

use super::common::run_container_command_with_optional_flag;

pub(super) fn wait(service: &ServiceConfig, condition: Option<&str>) -> Result<()> {
    run_container_command_with_optional_flag(
        service,
        "wait",
        "--condition",
        condition,
        &crate::docker::runtime_command_error_context("wait"),
    )
}
