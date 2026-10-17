//! recreate random-ports execution and persistence flow.

use anyhow::Result;
use std::path::Path;

use crate::{cli, config};

use super::runtime::RecreateRuntimeContext;

pub(super) fn run_recreate_flow<F>(
    planned: &[config::ServiceConfig],
    recreate_context: RecreateRuntimeContext<'_>,
    env_path: Option<&Path>,
    parallel: usize,
    runtime_runner: F,
) -> Result<()>
where
    F: Fn(&config::ServiceConfig, &RecreateRuntimeContext<'_>) -> Result<()> + Sync,
{
    cli::support::run_random_ports_flow(
        planned,
        parallel,
        |runtime| runtime.kind == config::Kind::App,
        |runtime| runtime_runner(runtime, &recreate_context),
        |runtime| runtime,
        cli::support::RandomPortsPersistenceOptions { env_path },
    )
}
