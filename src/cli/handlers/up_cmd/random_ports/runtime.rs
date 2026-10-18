//! up random-ports runtime start helpers.

use anyhow::Result;

use crate::cli::handlers::log;
use crate::{cli, config, docker};

#[derive(Clone, Copy)]
pub(super) struct StartRuntimeContext<'a> {
    pub(super) quiet: bool,
    pub(super) start_context: cli::support::ServiceStartContext<'a>,
}

impl<'a> StartRuntimeContext<'a> {
    pub(super) fn new(quiet: bool, start_context: cli::support::ServiceStartContext<'a>) -> Self {
        Self {
            quiet,
            start_context,
        }
    }
}

pub(super) fn start_runtime_service(
    runtime: &config::ServiceConfig,
    uses_random_port: bool,
    context: &StartRuntimeContext<'_>,
    healthy: bool,
    timeout: Option<u64>,
    pull_policy: docker::PullPolicy,
    recreate: bool,
) -> Result<()> {
    log::info_if_not_quiet(
        context.quiet,
        &runtime.name,
        &format!(
            "Starting service on port {} ({})",
            runtime.port,
            if uses_random_port {
                "random"
            } else {
                "explicit"
            }
        ),
    );
    cli::support::start_service(
        runtime,
        &context.start_context,
        recreate,
        pull_policy,
        healthy,
        timeout,
        true,
    )
}
