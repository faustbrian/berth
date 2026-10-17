//! start command core orchestration flow.

use anyhow::Result;
use std::path::Path;

use crate::cli::args::PortStrategyArg;
use crate::{cli, config, docker};

use super::start_bootstrap::{StartBootstrapOptions, run_start_bootstrap};

pub(super) struct StartFlowOptions<'a> {
    pub(super) service: Option<&'a str>,
    pub(super) kind: Option<config::Kind>,
    pub(super) profile: Option<&'a str>,
    pub(super) wait: bool,
    pub(super) no_wait: bool,
    pub(super) wait_timeout: u64,
    pub(super) pull_policy: docker::PullPolicy,
    pub(super) force_recreate: bool,
    pub(super) include_project_deps: bool,
    pub(super) parallel: usize,
    pub(super) quiet: bool,
    pub(super) no_color: bool,
    pub(super) dry_run: bool,
    pub(super) repro: bool,
    pub(super) runtime_env: Option<&'a str>,
    pub(super) config_path: Option<&'a Path>,
    pub(super) project_root: Option<&'a Path>,
}

pub(super) fn run_start_flow(
    config: &mut config::Config,
    options: StartFlowOptions<'_>,
) -> Result<()> {
    cli::support::run_doctor(
        config,
        cli::support::RunDoctorOptions {
            fix: false,
            repro: options.repro,
            reachability: false,
            allow_stopped_app_runtime_checks: true,
            config_path: options.config_path,
            project_root: options.project_root,
        },
    )?;

    run_up_from_start(config, &options)?;

    if config.project_type == config::ProjectType::Library {
        return Ok(());
    }

    run_start_bootstrap(
        config,
        StartBootstrapOptions {
            service: options.service,
            kind: options.kind,
            profile: options.profile,
            config_path: options.config_path,
            project_root: options.project_root,
            quiet: options.quiet,
        },
    )
}

fn run_up_from_start(config: &mut config::Config, options: &StartFlowOptions<'_>) -> Result<()> {
    cli::handlers::handle_up(
        config,
        cli::handlers::HandleUpOptions {
            service: options.service,
            kind: options.kind,
            profile: options.profile,
            wait: options.wait,
            no_wait: options.no_wait,
            wait_timeout: options.wait_timeout,
            pull_policy: options.pull_policy,
            force_recreate: options.force_recreate,
            publish_all: false,
            no_publish_all: false,
            port_strategy: PortStrategyArg::Random,
            port_seed: None,
            env_output: false,
            include_project_deps: options.include_project_deps,
            seed: false,
            parallel: options.parallel,
            quiet: options.quiet,
            no_color: options.no_color,
            dry_run: options.dry_run,
            repro: options.repro,
            runtime_env: options.runtime_env,
            config_path: options.config_path,
            project_root: options.project_root,
        },
    )
}
