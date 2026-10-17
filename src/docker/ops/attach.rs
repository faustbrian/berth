//! docker ops attach module.
//!
//! Contains docker attach operation used by Berth command workflows.

use anyhow::Result;

use crate::config::ServiceConfig;

use super::common::{push_flag, push_option, run_docker_status};

pub(super) fn attach(
    service: &ServiceConfig,
    no_stdin: bool,
    sig_proxy: bool,
    detach_keys: Option<&str>,
) -> Result<()> {
    let container_name = service.container_name()?;
    let mut args = vec!["attach".to_owned()];
    push_flag(&mut args, no_stdin, "--no-stdin");
    push_flag(&mut args, sig_proxy, "--sig-proxy=true");
    push_option(&mut args, "--detach-keys", detach_keys);
    args.push(container_name);
    run_docker_status(
        &args,
        &crate::docker::runtime_command_error_context("attach"),
    )
}
