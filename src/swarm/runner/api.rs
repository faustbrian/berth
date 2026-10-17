//! Public swarm runner API surface used by CLI handlers.

use anyhow::Result;
use std::path::Path;

use super::{args, project_deps, run};

use crate::cli::args::PortStrategyArg;
pub(crate) use run::RunSwarmOptions;
pub(crate) use run::run_swarm;

#[cfg_attr(not(test), allow(dead_code))]
/// Builds the child CLI argument vector for a single swarm target.
///
/// This is exported for tests and for modules that need deterministic child
/// process invocation behavior.
#[expect(
    clippy::too_many_arguments,
    reason = "7.0.0 signature kept as is; it predates the 9.x lint baseline (L154)"
)]
pub(crate) fn swarm_child_args(
    target: &super::super::targets::ResolvedSwarmTarget,
    command: &[String],
    port_strategy: PortStrategyArg,
    port_seed: Option<&str>,
    env_output: bool,
    quiet: bool,
    no_color: bool,
    dry_run: bool,
    runtime_env: Option<&str>,
) -> Vec<String> {
    args::swarm_child_args(
        target,
        command,
        port_strategy,
        port_seed,
        env_output,
        quiet,
        no_color,
        dry_run,
        runtime_env,
    )
}

/// Runs dependency targets declared by the current project's swarm context.
pub(crate) struct RunProjectSwarmDependenciesOptions<'a> {
    pub(crate) operation: &'a str,
    pub(crate) project_root: &'a Path,
    pub(crate) quiet: bool,
    pub(crate) no_color: bool,
    pub(crate) dry_run: bool,
    pub(crate) runtime_env: Option<&'a str>,
    pub(crate) force_down_deps: bool,
}

pub(crate) fn run_project_swarm_dependencies(
    options: RunProjectSwarmDependenciesOptions<'_>,
) -> Result<()> {
    project_deps::run_project_swarm_dependencies(options)
}
