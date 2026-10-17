//! docker ops port module.
//!
//! Contains docker port operation used by Berth command workflows.

use anyhow::Result;

use crate::config::ServiceConfig;
use crate::docker::is_dry_run;

use super::common::{run_docker_output, run_docker_status};

pub(super) fn port(service: &ServiceConfig, private_port: Option<&str>) -> Result<()> {
    let args = build_port_args(service, private_port)?;
    run_docker_status(&args, &crate::docker::runtime_command_error_context("port"))
}

pub(super) fn port_output(service: &ServiceConfig, private_port: Option<&str>) -> Result<String> {
    let args = build_port_args(service, private_port)?;
    if is_dry_run() {
        return Ok(String::new());
    }

    let output = run_docker_output(&args, &crate::docker::runtime_command_error_context("port"))?;
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn build_port_args(service: &ServiceConfig, private_port: Option<&str>) -> Result<Vec<String>> {
    let container_name = service.container_name()?;
    let mut args = vec!["port".to_owned(), container_name];
    if let Some(port_name) = private_port {
        args.push(port_name.to_owned());
    }
    Ok(args)
}
