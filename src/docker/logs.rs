//! docker logs module.
//!
//! Contains docker logs logic used by Berth command workflows.

use anyhow::Result;

use crate::config::ServiceConfig;

use super::{is_dry_run, print_docker_command};
use args::build_logs_args;
use run::run_docker_status;
use stream::stream_logs_with_prefix;

mod args;
mod many;
mod run;
mod stream;

#[derive(Clone, Debug)]
pub struct LogsOptions {
    pub follow: bool,
    pub tail: Option<u64>,
    pub since: Option<String>,
    pub until: Option<String>,
    pub timestamps: bool,
    pub prefix: bool,
    /// One JSON object per line (`{"service", "stream", "line"}`) instead
    /// of raw lines.
    pub json: bool,
}

pub fn logs(service: &ServiceConfig, options: LogsOptions) -> Result<()> {
    if options.json {
        return logs_prefixed(service, options);
    }
    let container_name = service.container_name()?;
    let args = build_logs_args(&container_name, options);

    if is_dry_run() {
        print_docker_command(&args);
        return Ok(());
    }

    let status = run_docker_status(&args, &super::runtime_command_error_context("logs"))?;

    if !status.success() {
        anyhow::bail!("Failed to get logs for container '{container_name}'");
    }

    Ok(())
}

pub fn logs_prefixed(service: &ServiceConfig, options: LogsOptions) -> Result<()> {
    let container_name = service.container_name()?;
    let json = options.json;
    let args = build_logs_args(&container_name, options);

    if is_dry_run() {
        print_docker_command(&args);
        return Ok(());
    }

    stream_logs_with_prefix(&args, &service.name, &container_name, json)
}

pub fn logs_many(services: &[ServiceConfig], options: LogsOptions) -> Result<()> {
    many::logs_many(services, options)
}
