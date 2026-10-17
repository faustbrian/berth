//! docker ops cp module.
//!
//! Contains docker cp operation used by Berth command workflows.

use anyhow::Result;

use super::CpOptions;
use super::common::{push_flag, run_docker_status};

pub(super) fn cp(source: &str, destination: &str, options: CpOptions) -> Result<()> {
    let mut args = vec!["cp".to_owned()];
    push_flag(&mut args, options.follow_link, "-L");
    push_flag(&mut args, options.archive, "-a");
    args.push(source.to_owned());
    args.push(destination.to_owned());
    run_docker_status(&args, &crate::docker::runtime_command_error_context("cp"))
}
