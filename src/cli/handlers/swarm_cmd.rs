//! cli handlers swarm cmd module.
//!
//! Contains cli handlers swarm cmd logic used by Berth command workflows.

use anyhow::Result;
use std::path::Path;

use crate::cli::args::PortStrategyArg;
use crate::{config, swarm};

pub(crate) struct HandleSwarmOptions<'a> {
    pub(crate) command: &'a [String],
    pub(crate) only: &'a [String],
    pub(crate) no_deps: bool,
    pub(crate) force: bool,
    pub(crate) parallel: usize,
    pub(crate) fail_fast: bool,
    pub(crate) port_strategy: PortStrategyArg,
    pub(crate) port_seed: Option<&'a str>,
    pub(crate) env_output: bool,
    pub(crate) quiet: bool,
    pub(crate) no_color: bool,
    pub(crate) dry_run: bool,
    pub(crate) repro: bool,
    pub(crate) runtime_env: Option<&'a str>,
    pub(crate) config_path: Option<&'a Path>,
    pub(crate) project_root: Option<&'a Path>,
}

pub(crate) fn handle_swarm(config: &config::Config, options: HandleSwarmOptions<'_>) -> Result<()> {
    // Refused once here instead of by every child `berth up`.
    if options.env_output {
        anyhow::bail!("{}", crate::cli::support::env_write_removed());
    }
    verify_repro_constraints(
        config,
        options.command,
        options.repro,
        options.config_path,
        options.project_root,
    )?;

    swarm::run_swarm(swarm::RunSwarmOptions {
        config,
        command: options.command,
        only: options.only,
        include_deps: !options.no_deps,
        force_down_deps: options.force,
        parallel: options.parallel,
        fail_fast: options.fail_fast,
        port_strategy: options.port_strategy,
        port_seed: options.port_seed,
        env_output: options.env_output,
        quiet: options.quiet,
        no_color: options.no_color,
        dry_run: options.dry_run,
        runtime_env: options.runtime_env,
        config_path: options.config_path,
        project_root: options.project_root,
        dependency_run: false,
    })
}

fn verify_repro_constraints(
    config: &config::Config,
    command: &[String],
    repro: bool,
    config_path: Option<&Path>,
    project_root: Option<&Path>,
) -> Result<()> {
    if !is_repro_up_invocation(command, repro) {
        return Ok(());
    }

    config::verify_lockfile_with(
        config,
        config::ProjectRootPathOptions::new(config_path, project_root),
    )
}

fn is_repro_up_invocation(command: &[String], repro: bool) -> bool {
    repro && command.first().is_some_and(|subcommand| subcommand == "up")
}
