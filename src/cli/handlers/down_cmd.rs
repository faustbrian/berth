//! cli handlers down cmd module.
//!
//! Contains cli handlers down cmd logic used by Berth command workflows.

use anyhow::Result;
use std::path::Path;

use super::log;
use crate::{cli, config, docker, serve};

pub(crate) struct HandleDownOptions<'a> {
    pub(crate) service: Option<&'a str>,
    pub(crate) services: &'a [String],
    pub(crate) kind: Option<config::Kind>,
    pub(crate) profile: Option<&'a str>,
    pub(crate) include_project_deps: bool,
    pub(crate) force: bool,
    pub(crate) timeout: u64,
    pub(crate) parallel: usize,
    pub(crate) quiet: bool,
    pub(crate) no_color: bool,
    pub(crate) dry_run: bool,
    pub(crate) runtime_env: Option<&'a str>,
    pub(crate) config_path: Option<&'a Path>,
    pub(crate) project_root: Option<&'a Path>,
}

pub(crate) fn handle_down(config: &config::Config, options: HandleDownOptions<'_>) -> Result<()> {
    let workspace_root =
        cli::support::workspace_with_project_deps(cli::support::WorkspaceWithProjectDepsOptions {
            operation: "down",
            config_path: options.config_path,
            project_root: options.project_root,
            include_project_deps: options.include_project_deps,
            quiet: options.quiet,
            no_color: options.no_color,
            dry_run: options.dry_run,
            runtime_env: options.runtime_env,
            force_down_deps: options.force,
        })?;
    let selected = super::service_scope::selected_services_in_scope(
        config,
        options.service,
        options.services,
        options.kind,
        options.profile,
    )?;
    cli::hooks::run_phase_hooks_for_services(
        &selected,
        config::HookPhase::PreDown,
        &workspace_root,
        options.quiet,
    )?;

    cli::support::run_selected_services(&selected, options.parallel, |svc| {
        stop_selected_service(svc, options.quiet, options.timeout)
    })?;
    crate::network::remove_unused_networks(selected.iter().copied())?;

    cli::hooks::run_phase_hooks_for_services(
        &selected,
        config::HookPhase::PostDown,
        &workspace_root,
        options.quiet,
    )?;

    Ok(())
}

fn stop_selected_service(service: &config::ServiceConfig, quiet: bool, timeout: u64) -> Result<()> {
    log::info_if_not_quiet(quiet, &service.name, "Stopping service");
    if service.kind == config::Kind::App {
        serve::down(service)
    } else {
        docker::down(service, timeout)
    }
}
