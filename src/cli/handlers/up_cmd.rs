//! cli handlers up cmd module.
//!
//! Contains cli handlers up cmd logic used by Berth command workflows.

use anyhow::Result;
use std::path::Path;

use crate::cli::args::PortStrategyArg;
use crate::config;
use crate::docker;

mod data_seed;
mod options;
mod post_actions;
mod preflight;
mod random_ports;
mod startup;

use data_seed::apply_data_seeds;
use options::resolve_execution_flags;
use post_actions::{PostUpActionsOptions, run_post_up_actions};
use preflight::{PrepareUpContextOptions, PreparedUpContext, prepare_up_context};
use random_ports::{RunRandomPortsUpOptions, run_random_ports_up};
use startup::RunStandardUpOptions;

pub(crate) struct HandleUpOptions<'a> {
    pub(crate) service: Option<&'a str>,
    pub(crate) kind: Option<config::Kind>,
    pub(crate) profile: Option<&'a str>,
    pub(crate) wait: bool,
    pub(crate) no_wait: bool,
    pub(crate) wait_timeout: u64,
    pub(crate) pull_policy: docker::PullPolicy,
    pub(crate) force_recreate: bool,
    pub(crate) publish_all: bool,
    pub(crate) no_publish_all: bool,
    pub(crate) port_strategy: PortStrategyArg,
    pub(crate) port_seed: Option<&'a str>,
    pub(crate) save_ports: bool,
    pub(crate) env_output: bool,
    pub(crate) include_project_deps: bool,
    pub(crate) seed: bool,
    pub(crate) parallel: usize,
    pub(crate) quiet: bool,
    pub(crate) no_color: bool,
    pub(crate) dry_run: bool,
    pub(crate) repro: bool,
    pub(crate) runtime_env: Option<&'a str>,
    pub(crate) config_path: Option<&'a Path>,
    pub(crate) project_root: Option<&'a Path>,
}

pub(crate) fn handle_up(config: &mut config::Config, options: HandleUpOptions<'_>) -> Result<()> {
    let PreparedUpContext {
        workspace_root,
        project_dependency_env,
        env_path,
    } = prepare_up_context(
        config,
        PrepareUpContextOptions {
            repro: options.repro,
            env_output: options.env_output,
            save_ports: options.save_ports,
            config_path: options.config_path,
            project_root: options.project_root,
            include_project_deps: options.include_project_deps,
            quiet: options.quiet,
            no_color: options.no_color,
            dry_run: options.dry_run,
            runtime_env: options.runtime_env,
        },
    )?;

    let execution_flags = resolve_execution_flags(
        options.wait,
        options.no_wait,
        options.publish_all,
        options.no_publish_all,
    );
    let use_wait = execution_flags.use_wait;
    let use_publish_all = execution_flags.use_publish_all;
    if use_publish_all {
        run_random_ports_up(
            config,
            RunRandomPortsUpOptions {
                service: options.service,
                kind: options.kind,
                profile: options.profile,
                healthy: use_wait,
                timeout: options.wait_timeout,
                pull_policy: options.pull_policy,
                recreate: options.force_recreate,
                force_random_ports: true,
                port_strategy: options.port_strategy,
                port_seed: options.port_seed,
                save_ports: options.save_ports,
                env_output: options.env_output,
                parallel: options.parallel,
                quiet: options.quiet,
                runtime_env: options.runtime_env,
                workspace_root: &workspace_root,
                project_dependency_env: &project_dependency_env,
                config_path: options.config_path,
                project_root: options.project_root,
            },
        )?;
    } else {
        startup::run_standard_up(
            config,
            RunStandardUpOptions {
                service: options.service,
                kind: options.kind,
                profile: options.profile,
                healthy: use_wait,
                timeout: options.wait_timeout,
                pull_policy: options.pull_policy,
                recreate: options.force_recreate,
                quiet: options.quiet,
                workspace_root: &workspace_root,
                project_dependency_env: &project_dependency_env,
                env_path: env_path.as_deref(),
            },
        )?;
    }

    run_post_up_actions(
        config,
        PostUpActionsOptions {
            service: options.service,
            kind: options.kind,
            profile: options.profile,
            seed: options.seed,
            workspace_root: &workspace_root,
            quiet: options.quiet,
        },
    )?;

    Ok(())
}
