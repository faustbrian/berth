//! docker ops top module.
//!
//! Contains docker top operation used by Berth command workflows.

use anyhow::Result;

use crate::config::ServiceConfig;

use super::common::run_docker_status;

pub(super) fn top(service: &ServiceConfig, top_args: &[String]) -> Result<()> {
    let container_name = service.container_name()?;
    let mut args = vec!["top".to_owned(), container_name];
    args.extend(top_args.iter().cloned());
    run_docker_status(&args, &crate::docker::runtime_command_error_context("top"))
}
