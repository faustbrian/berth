//! cli handlers up cmd startup module.
//!
//! Contains cli handlers up cmd startup logic used by Berth command workflows.

use anyhow::Result;
use std::collections::HashMap;
use std::path::Path;

use crate::cli::handlers::log;
use crate::{cli, config, docker, env};

pub(super) struct RunStandardUpOptions<'a> {
    pub(super) service: Option<&'a str>,
    pub(super) kind: Option<config::Kind>,
    pub(super) profile: Option<&'a str>,
    pub(super) healthy: bool,
    pub(super) timeout: u64,
    pub(super) pull_policy: docker::PullPolicy,
    pub(super) recreate: bool,
    pub(super) quiet: bool,
    pub(super) workspace_root: &'a Path,
    pub(super) project_dependency_env: &'a HashMap<String, String>,
    pub(super) env_path: Option<&'a Path>,
}

pub(super) fn run_standard_up(
    config: &mut config::Config,
    options: RunStandardUpOptions<'_>,
) -> Result<()> {
    let startup_services =
        cli::support::resolve_up_services(config, options.service, options.kind, options.profile)?;
    let app_env = cli::support::runtime_app_env(config, options.project_dependency_env)?;
    let start_context = cli::support::ServiceStartContext::new(options.workspace_root, &app_env);
    for svc in startup_services {
        start_runtime_service(
            svc,
            &start_context,
            options.recreate,
            options.pull_policy,
            options.healthy,
            options.timeout,
            options.env_path,
            options.quiet,
        )?;
    }

    Ok(())
}

fn start_runtime_service(
    service: &config::ServiceConfig,
    start_context: &cli::support::ServiceStartContext<'_>,
    recreate: bool,
    pull_policy: docker::PullPolicy,
    wait_healthy: bool,
    health_timeout_secs: u64,
    env_path: Option<&Path>,
    quiet: bool,
) -> Result<()> {
    log::info_if_not_quiet(quiet, &service.name, "Starting service");
    cli::support::start_service(
        service,
        start_context,
        recreate,
        pull_policy,
        wait_healthy,
        health_timeout_secs,
        true,
    )?;
    if let Some(path) = env_path {
        env::update_env(service, path, true)?;
    }
    Ok(())
}
