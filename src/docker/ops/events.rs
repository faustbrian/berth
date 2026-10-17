//! docker ops events module.
//!
//! Contains docker events operation used by Berth command workflows.

use anyhow::Result;

use super::common::{push_option, run_docker_status};

pub(super) fn events(
    since: Option<&str>,
    until: Option<&str>,
    format: Option<&str>,
    filters: &[String],
) -> Result<()> {
    let mut args = vec!["events".to_owned()];
    push_option(&mut args, "--since", since);
    push_option(&mut args, "--until", until);
    push_option(&mut args, "--format", format);
    for filter in filters {
        args.push("--filter".to_owned());
        args.push(filter.clone());
    }
    run_docker_status(
        &args,
        &crate::docker::runtime_command_error_context("events"),
    )
}
