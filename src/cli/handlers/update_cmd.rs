//! cli handlers update cmd module.
//!
//! Contains cli handlers update cmd logic used by Berth command workflows.

use anyhow::Result;
use std::path::Path;

use crate::output::{self, LogLevel, Persistence};
use crate::{cli, config, docker};

pub(crate) struct HandleUpdateOptions<'a> {
    pub(crate) service: Option<&'a str>,
    pub(crate) kind: Option<config::Kind>,
    pub(crate) profile: Option<&'a str>,
    pub(crate) force_recreate: bool,
    pub(crate) no_build: bool,
    pub(crate) wait: bool,
    pub(crate) wait_timeout: u64,
    pub(crate) config_path: Option<&'a Path>,
    pub(crate) project_root: Option<&'a Path>,
}

pub(crate) fn handle_update(
    config: &config::Config,
    options: HandleUpdateOptions<'_>,
) -> Result<()> {
    let runtime = cli::support::resolve_project_runtime_context(
        config,
        options.config_path,
        options.project_root,
    )?;
    let services =
        cli::support::resolve_up_services(config, options.service, options.kind, options.profile)?;
    let start_context = runtime.service_start_context();
    for svc in services {
        update_service(
            svc,
            &start_context,
            options.force_recreate,
            options.wait,
            options.wait_timeout,
            !options.no_build,
        )?;
    }
    Ok(())
}

fn update_service(
    service: &config::ServiceConfig,
    start_context: &cli::support::ServiceStartContext<'_>,
    recreate: bool,
    wait_healthy: bool,
    health_timeout_secs: u64,
    build: bool,
) -> Result<()> {
    output::event(
        &service.name,
        LogLevel::Info,
        "Updating service",
        Persistence::Persistent,
    );
    docker::pull(service)?;
    cli::support::start_service(
        service,
        start_context,
        recreate,
        docker::PullPolicy::Never,
        wait_healthy,
        health_timeout_secs,
        build,
    )
}
