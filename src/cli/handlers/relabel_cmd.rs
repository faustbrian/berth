//! cli handlers relabel cmd module.
//!
//! Contains cli handlers relabel cmd logic used by Berth command workflows.

use anyhow::Result;
use std::path::Path;

use super::service_scope::for_each_service_in_scope;
use crate::output::{self, LogLevel, Persistence};
use crate::{cli, config, docker};

enum RelabelAction {
    Skip(String),
    Recreate {
        container_name: String,
        status: String,
    },
}

pub(crate) struct HandleRelabelOptions<'a> {
    pub(crate) service: Option<&'a str>,
    pub(crate) services: &'a [String],
    pub(crate) kind: Option<config::Kind>,
    pub(crate) profile: Option<&'a str>,
    pub(crate) wait: bool,
    pub(crate) wait_timeout: Option<u64>,
    pub(crate) parallel: usize,
    pub(crate) config_path: Option<&'a Path>,
    pub(crate) project_root: Option<&'a Path>,
}

pub(crate) fn handle_relabel(
    config: &config::Config,
    options: HandleRelabelOptions<'_>,
) -> Result<()> {
    let runtime = cli::support::resolve_project_runtime_context(
        config,
        options.config_path,
        options.project_root,
    )?;
    let start_context = runtime.service_start_context();

    for_each_service_in_scope(
        config,
        options.service,
        options.services,
        options.kind,
        options.profile,
        options.parallel,
        |svc| match determine_relabel_action(svc)? {
            RelabelAction::Skip(message) => {
                output::event(&svc.name, LogLevel::Info, &message, Persistence::Persistent);
                Ok(())
            }
            RelabelAction::Recreate {
                container_name,
                status,
            } => apply_relabel(
                svc,
                &container_name,
                &status,
                &start_context,
                options.wait,
                options.wait_timeout,
            ),
        },
    )
}

fn determine_relabel_action(service: &config::ServiceConfig) -> Result<RelabelAction> {
    let container_name = service.container_name()?;
    let Some(status) = docker::inspect_status(&container_name) else {
        return Ok(RelabelAction::Skip(format!(
            "Skipped relabel because container {container_name} was not found"
        )));
    };

    let managed = docker::inspect_label(&container_name, docker::LABEL_MANAGED)
        .is_some_and(|value| value == docker::VALUE_MANAGED_TRUE);
    if managed {
        return Ok(RelabelAction::Skip(format!(
            "Skipped relabel because container {container_name} already has Berth labels"
        )));
    }

    Ok(RelabelAction::Recreate {
        container_name,
        status,
    })
}

fn apply_relabel(
    service: &config::ServiceConfig,
    container_name: &str,
    status: &str,
    start_context: &cli::support::ServiceStartContext<'_>,
    wait: bool,
    wait_timeout: Option<u64>,
) -> Result<()> {
    output::event(
        &service.name,
        LogLevel::Info,
        &format!("Recreating container {container_name} to apply Berth labels (status: {status})"),
        Persistence::Persistent,
    );

    cli::support::recreate_service(service, start_context, wait, wait_timeout)?;

    output::event(
        &service.name,
        LogLevel::Success,
        &format!("Applied Berth labels to container {container_name}"),
        Persistence::Persistent,
    );
    Ok(())
}
