//! docker ops stats module.
//!
//! Contains docker stats operation used by Berth command workflows.

use anyhow::Result;

use crate::config::ServiceConfig;

use super::StatsOptions;
use super::common::{push_flag, push_option, run_docker_status};

pub(super) fn stats(service: &ServiceConfig, options: StatsOptions<'_>) -> Result<()> {
    let container_name = service.container_name()?;
    let mut args = vec!["stats".to_owned()];
    push_flag(&mut args, options.no_stream, "--no-stream");
    push_option(&mut args, "--format", options.format);
    args.push(container_name);
    run_docker_status(
        &args,
        &crate::docker::runtime_command_error_context("stats"),
    )
}
