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
    pub(crate) purge: bool,
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
        if options.purge {
            // `rm -v` also deletes anonymous volumes the image declares
            // (`VOLUME`), which no name would find afterwards.
            docker::remove_container_with_volumes(svc)?;
        }
        stop_selected_service(svc, options.quiet, options.timeout)
    })?;
    // Purging the whole instance also deletes its local state (managed
    // credentials, the APP_KEY); a subset keeps it for the services left.
    // A runtime environment (`--env`) shares the instance's state, so only
    // the default environment's purge deletes it.
    let whole_instance = options.service.is_none()
        && options.services.is_empty()
        && options.kind.is_none()
        && options.profile.is_none()
        && match options.runtime_env {
            Some(name) => config::is_default_runtime_env_name(name)?,
            None => true,
        };
    crate::network::remove_unused_networks(selected.iter().copied())?;

    // Data goes last, after containers and networks, so a volume that
    // cannot be removed never leaves a network behind or skips hooks; every
    // failure is reported once the rest is done.
    let mut purge_error = None;
    if options.purge {
        for service in &selected {
            if let Err(error) = docker::remove_service_volumes(service) {
                purge_error.get_or_insert(error);
            }
        }
        if whole_instance && let Err(error) = purge_instance(config) {
            purge_error.get_or_insert(error);
        }
    }

    cli::hooks::run_phase_hooks_for_services(
        &selected,
        config::HookPhase::PostDown,
        &workspace_root,
        options.quiet,
    )?;

    purge_error.map_or(Ok(()), Err)
}

fn stop_selected_service(service: &config::ServiceConfig, quiet: bool, timeout: u64) -> Result<()> {
    log::info_if_not_quiet(quiet, &service.name, "Stopping service");
    if service.kind == config::Kind::App {
        serve::down(service)
    } else {
        docker::down(service, timeout)
    }
}

/// Purges the whole instance: every runtime environment's containers,
/// networks and data, then its berth state (credentials, `APP_KEY`).
fn purge_instance(config: &config::Config) -> Result<()> {
    let Some(runtime) = config.service.first().map(|service| &service.runtime) else {
        return Ok(());
    };
    let (Some(project), Some(instance)) = (&runtime.project, &runtime.instance) else {
        return Ok(());
    };
    if docker::is_dry_run() {
        log::info_if_not_quiet(
            false,
            "down",
            &format!("[dry-run] Delete berth state of {instance}"),
        );
        return Ok(());
    }
    // Each step runs even when an earlier one fails, so one stubborn object
    // never leaves the rest behind; the first failure is reported.
    // The instance's berth state goes even when its tenants stay, so only
    // this checkout's next purge (they are matched by name) can drop them.
    let tenants = crate::shared::drop_instance_tenants(project, instance).map_err(purge_retry);
    let objects = docker::purge_instance_objects(project, instance);
    let state = crate::home::remove_instance_state(project, instance);
    tenants.and(objects).and(state)
}
/// Tells how to drop tenants a purge left behind, unless the cause is a
/// corrupt admin secret: that carries its own recovery, and purging again
/// cannot fix it.
fn purge_retry(error: anyhow::Error) -> anyhow::Error {
    if crate::home::is_invalid_secret(&error) {
        return error;
    }
    anyhow::anyhow!(
        "{error:#}; those tenants stay on the shared server: fix the cause and run {} again",
        crate::followup::command("down --purge")
    )
}

#[cfg(test)]
mod tests {
    use super::purge_retry;

    #[test]
    fn a_purge_retry_names_the_instance_but_not_for_a_corrupt_secret() {
        crate::followup::record(crate::followup::Invocation {
            instance: Some("agent-2".to_owned()),
            ..crate::followup::Invocation::default()
        });
        let stopped = purge_retry(anyhow::anyhow!("server stopped"));
        assert!(
            stopped
                .to_string()
                .contains("run `berth --instance agent-2 down --purge` again"),
            "{stopped}"
        );
        let corrupt = anyhow::Error::new(crate::home::InvalidSecret {
            path: "/home/admin".into(),
        })
        .context("purging (`down --purge`) does not fix this")
        .context("failed to drop tenants of agent-2");
        let message = format!("{:#}", purge_retry(corrupt));
        assert!(!message.contains("again"), "{message}");
        crate::followup::record(crate::followup::Invocation::default());
    }
}
