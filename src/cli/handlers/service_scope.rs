//! Shared service selection/execution helpers for CLI handlers.

use anyhow::Result;

use super::log;
use crate::{cli, config};

pub(super) fn for_each_service<F>(
    config: &config::Config,
    service: Option<&str>,
    kind: Option<config::Kind>,
    parallel: usize,
    callback: F,
) -> Result<()>
where
    F: Fn(&config::ServiceConfig) -> Result<()> + Send + Sync,
{
    cli::support::ServiceScope::new(config, service, kind).for_each(parallel, callback)
}

pub(super) fn selected_services<'a>(
    config: &'a config::Config,
    service: Option<&'a str>,
    kind: Option<config::Kind>,
) -> Result<Vec<&'a config::ServiceConfig>> {
    cli::support::ServiceScope::new(config, service, kind).selected()
}

pub(super) fn selected_services_in_scope<'a>(
    config: &'a config::Config,
    service: Option<&'a str>,
    services: &'a [String],
    kind: Option<config::Kind>,
    profile: Option<&'a str>,
) -> Result<Vec<&'a config::ServiceConfig>> {
    cli::support::selected_services_with_filters(config, service, services, kind, None, profile)
}

pub(super) fn for_each_service_in_scope<F>(
    config: &config::Config,
    service: Option<&str>,
    services: &[String],
    kind: Option<config::Kind>,
    profile: Option<&str>,
    parallel: usize,
    callback: F,
) -> Result<()>
where
    F: Fn(&config::ServiceConfig) -> Result<()> + Send + Sync,
{
    let selected = selected_services_in_scope(config, service, services, kind, profile)?;
    cli::support::run_selected_services(&selected, parallel, |svc| callback(svc))
}

#[expect(
    clippy::too_many_arguments,
    reason = "7.0.0 signature kept as is; it predates the 9.x lint baseline (L154)"
)]
pub(super) fn for_each_service_in_scope_with_info<F>(
    config: &config::Config,
    service: Option<&str>,
    services: &[String],
    kind: Option<config::Kind>,
    profile: Option<&str>,
    parallel: usize,
    quiet: bool,
    message: &str,
    callback: F,
) -> Result<()>
where
    F: Fn(&config::ServiceConfig) -> Result<()> + Send + Sync,
{
    for_each_service_in_scope(
        config,
        service,
        services,
        kind,
        profile,
        parallel,
        |service| {
            log::info_if_not_quiet(quiet, &service.name, message);
            callback(service)
        },
    )
}
