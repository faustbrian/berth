//! cli handlers up cmd random ports module.
//!
//! Contains cli handlers up cmd random ports logic used by Berth command workflows.

use anyhow::Result;
use std::collections::HashMap;
use std::path::Path;

use crate::cli::args::PortStrategyArg;
use crate::config;
use crate::docker;

mod flow;
mod planning;
mod port_assignment;
mod preflight;
mod runtime;

use flow::run_up_flow;
use preflight::{PrepareRandomPortsUpOptions, PreparedRandomPortsUp, prepare_random_ports_up};
use runtime::{StartRuntimeContext, start_runtime_service};

pub(super) struct RunRandomPortsUpOptions<'a> {
    pub(super) service: Option<&'a str>,
    pub(super) kind: Option<config::Kind>,
    pub(super) profile: Option<&'a str>,
    pub(super) healthy: bool,
    pub(super) timeout: u64,
    pub(super) pull_policy: docker::PullPolicy,
    pub(super) recreate: bool,
    pub(super) force_random_ports: bool,
    pub(super) port_strategy: PortStrategyArg,
    pub(super) port_seed: Option<&'a str>,
    pub(super) env_output: bool,
    pub(super) parallel: usize,
    pub(super) quiet: bool,
    pub(super) runtime_env: Option<&'a str>,
    pub(super) workspace_root: &'a Path,
    pub(super) project_dependency_env: &'a HashMap<String, String>,
    pub(super) config_path: Option<&'a Path>,
    pub(super) project_root: Option<&'a Path>,
}

pub(super) fn run_random_ports_up(
    config: &mut config::Config,
    options: RunRandomPortsUpOptions<'_>,
) -> Result<()> {
    let PreparedRandomPortsUp {
        planned,
        app_env,
        env_path,
    } = prepare_random_ports_up(
        config,
        PrepareRandomPortsUpOptions {
            service: options.service,
            kind: options.kind,
            profile: options.profile,
            force_random_ports: options.force_random_ports,
            port_strategy: options.port_strategy,
            port_seed: options.port_seed,
            env_output: options.env_output,
            runtime_env: options.runtime_env,
            workspace_root: options.workspace_root,
            project_dependency_env: options.project_dependency_env,
            config_path: options.config_path,
            project_root: options.project_root,
        },
    )?;
    let start_context = StartRuntimeContext::new(
        options.quiet,
        crate::cli::support::ServiceStartContext::new(options.workspace_root, &app_env),
    );

    run_up_flow(
        &planned,
        options.parallel,
        env_path.as_deref(),
        |runtime, uses_random_port| {
            start_runtime_service(
                runtime,
                uses_random_port,
                &start_context,
                options.healthy,
                options.timeout,
                options.pull_policy,
                options.recreate,
            )
        },
    )?;

    Ok(())
}
