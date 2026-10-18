//! recreate random-ports runtime execution helpers.

use anyhow::Result;

use crate::cli::handlers::log;
use crate::{cli, config};

#[derive(Clone, Copy)]
pub(super) struct RecreateRuntimeContext<'a> {
    pub(super) healthy: bool,
    pub(super) timeout: Option<u64>,
    pub(super) start_context: cli::support::ServiceStartContext<'a>,
    pub(super) quiet: bool,
}

impl<'a> RecreateRuntimeContext<'a> {
    pub(super) fn new(
        healthy: bool,
        timeout: Option<u64>,
        start_context: cli::support::ServiceStartContext<'a>,
        quiet: bool,
    ) -> Self {
        Self {
            healthy,
            timeout,
            start_context,
            quiet,
        }
    }
}

pub(super) fn recreate_runtime(
    runtime: &config::ServiceConfig,
    context: &RecreateRuntimeContext<'_>,
) -> Result<()> {
    log::info_if_not_quiet(
        context.quiet,
        &runtime.name,
        &format!("Recreating service on random port {}", runtime.port),
    );

    cli::support::recreate_service(
        runtime,
        &context.start_context,
        context.healthy,
        context.timeout,
    )
}
