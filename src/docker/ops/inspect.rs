//! docker ops inspect module.
//!
//! Contains docker inspect operation used by Berth command workflows.

use anyhow::Result;

use crate::config::ServiceConfig;

use super::common::{push_flag, push_option, run_docker_status};

pub(super) fn inspect_container(
    service: &ServiceConfig,
    format: Option<&str>,
    size: bool,
    object_type: Option<&str>,
) -> Result<()> {
    let container_name = service.container_name()?;
    let mut args = vec!["inspect".to_owned()];
    push_option(&mut args, "--format", format);
    push_flag(&mut args, size, "--size");
    push_option(&mut args, "--type", object_type);
    args.push(container_name);
    run_docker_status(
        &args,
        &crate::docker::runtime_command_error_context("inspect"),
    )
}
