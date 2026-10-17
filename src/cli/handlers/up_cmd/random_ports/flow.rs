//! up random-port runtime execution and persistence flow.

use anyhow::Result;
use std::path::Path;

use crate::{cli, config};

pub(super) fn run_up_flow<F>(
    planned: &[(config::ServiceConfig, bool)],
    parallel: usize,
    env_path: Option<&Path>,
    runtime_runner: F,
) -> Result<()>
where
    F: Fn(&config::ServiceConfig, bool) -> Result<()> + Sync,
{
    cli::support::run_random_ports_flow(
        planned,
        parallel,
        |(runtime, _)| runtime.kind == config::Kind::App,
        |(runtime, uses_random_port)| runtime_runner(runtime, *uses_random_port),
        |(runtime, _)| runtime,
        cli::support::RandomPortsPersistenceOptions { env_path },
    )
}
