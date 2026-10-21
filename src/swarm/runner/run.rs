//! End-to-end swarm command orchestration.
//!
//! This module validates invocation, resolves targets, runs each target command,
//! and returns a consolidated success/failure result.

use anyhow::{Context, Result};
use std::path::Path;

use crate::cli::args::PortStrategyArg;
use crate::output::LogLevel;

use super::super::target_exec::{OutputMode, run_swarm_target};
use super::{
    execution,
    planning::{handle_ls_command, resolve_execution_targets, validate_swarm_invocation},
    summary,
};

pub(crate) struct RunSwarmOptions<'a> {
    pub(crate) config: &'a crate::config::Config,
    pub(crate) command: &'a [String],
    pub(crate) only: &'a [String],
    pub(crate) include_deps: bool,
    pub(crate) force_down_deps: bool,
    pub(crate) parallel: usize,
    pub(crate) fail_fast: bool,
    pub(crate) port_strategy: PortStrategyArg,
    pub(crate) port_seed: Option<&'a str>,
    pub(crate) env_output: bool,
    pub(crate) quiet: bool,
    pub(crate) no_color: bool,
    pub(crate) dry_run: bool,
    pub(crate) runtime_env: Option<&'a str>,
    pub(crate) config_path: Option<&'a Path>,
    pub(crate) project_root: Option<&'a Path>,
    /// Whether this run brings up a workspace member's dependencies (from
    /// its own `up` or `down`), not a `berth swarm` command.
    pub(crate) dependency_run: bool,
}

/// Executes a swarm command across resolved targets.
///
/// Behavior notes:
/// - `swarm ls` is handled as a read-only listing shortcut.
/// - `swarm ps` uses passthrough output to preserve tabular formatting.
/// - Other subcommands use structured logging output.
pub(crate) fn run_swarm(options: RunSwarmOptions<'_>) -> Result<()> {
    let subcommand = validate_swarm_invocation(options.command, options.parallel)?;
    let workspace_root =
        crate::cli::support::workspace_root(options.config_path, options.project_root)?;

    if handle_ls_command(
        options.config,
        &workspace_root,
        options.only,
        subcommand,
        options.command.len(),
    )? {
        return Ok(());
    }

    let targets = resolve_execution_targets(
        options.config,
        &workspace_root,
        options.command,
        options.only,
        options.include_deps,
        options.force_down_deps,
        subcommand,
        options.quiet,
    )?;

    let berth_executable =
        std::env::current_exe().context("failed to resolve current executable")?;
    let output_mode = output_mode_for_subcommand(subcommand);
    emit_swarm_start(options.command, targets.len(), output_mode, options.quiet);

    let (results, cancelled) = execution::run_targets(execution::RunTargetsOptions {
        targets: &targets,
        command: options.command,
        output_mode,
        parallel: options.parallel,
        fail_fast: options.fail_fast,
        port_strategy: options.port_strategy,
        port_seed: options.port_seed,
        env_output: options.env_output,
        quiet: options.quiet,
        no_color: options.no_color,
        dry_run: options.dry_run,
        runtime_env: options.runtime_env,
        berth_executable: &berth_executable,
        run_target: run_swarm_target,
    })?;

    summary::summarize_results(
        &targets,
        &results,
        cancelled,
        output_mode,
        &summary::Rerun {
            workspace_root: &workspace_root,
            runtime_env: options.runtime_env,
            include_deps: options.include_deps,
            force_down_deps: options.force_down_deps,
            port_strategy: options.port_strategy,
            port_seed: options.port_seed,
            parallel: options.parallel,
            fail_fast: options.fail_fast,
            command: options.command,
            dependency_run: options.dependency_run,
        },
    )
}

fn output_mode_for_subcommand(subcommand: &str) -> OutputMode {
    if subcommand == "ps" {
        OutputMode::Passthrough
    } else {
        OutputMode::Logged
    }
}

fn emit_swarm_start(command: &[String], target_count: usize, output_mode: OutputMode, quiet: bool) {
    if quiet {
        return;
    }

    let message = format!(
        "Running `berth {}` across {} target(s)",
        command.join(" "),
        target_count
    );
    super::output::emit_swarm(output_mode, LogLevel::Info, &message);
}
