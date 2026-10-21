//! Project-level swarm dependency execution.
//!
//! This module resolves the workspace context for the current project and runs
//! declared dependency targets before/after primary lifecycle commands.

use anyhow::Result;

use super::super::injection::resolve_workspace_swarm_context;
use super::api::RunProjectSwarmDependenciesOptions;

/// Runs dependency targets declared by the current project's swarm context.
///
/// The dependency run is intentionally serialized (`parallel=1`, `fail_fast=true`)
/// to keep dependency ordering predictable and diagnostics easy to follow.
pub(crate) fn run_project_swarm_dependencies(
    options: RunProjectSwarmDependenciesOptions<'_>,
) -> Result<()> {
    let Some(context) = resolve_workspace_swarm_context(options.project_root)? else {
        return Ok(());
    };

    if context.target.depends_on.is_empty() {
        emit_swarm_info(
            options.quiet,
            &format!(
                "No workspace dependencies configured for project '{}'",
                context.target.name
            ),
        );
        return Ok(());
    }

    emit_swarm_info(
        options.quiet,
        &format!(
            "Running project dependencies for '{}' from workspace {}",
            context.target.name,
            context.workspace_root.display()
        ),
    );

    let mut workspace_config = context.workspace_config.clone();
    if let Some(runtime_env) = options.runtime_env {
        crate::config::apply_runtime_env(&mut workspace_config, runtime_env)?;
    }

    let command = vec![options.operation.to_owned()];
    super::run_swarm(super::run::RunSwarmOptions {
        config: &workspace_config,
        command: &command,
        only: &context.target.depends_on,
        include_deps: true,
        force_down_deps: options.force_down_deps,
        parallel: 1,
        fail_fast: true,
        port_strategy: crate::cli::args::PortStrategyArg::Random,
        port_seed: None,
        env_output: false,
        quiet: options.quiet,
        no_color: options.no_color,
        dry_run: options.dry_run,
        runtime_env: options.runtime_env,
        config_path: None,
        project_root: Some(context.workspace_root.as_path()),
        dependency_run: true,
    })
}

fn emit_swarm_info(quiet: bool, message: &str) {
    if quiet {
        return;
    }
    super::output::emit_swarm(
        crate::swarm::target_exec::OutputMode::Logged,
        crate::output::LogLevel::Info,
        message,
    );
}
