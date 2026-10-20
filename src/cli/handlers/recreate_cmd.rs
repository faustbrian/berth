//! cli handlers recreate cmd module.
//!
//! Contains cli handlers recreate cmd logic used by Berth command workflows.

use anyhow::Result;
use std::path::Path;

use super::service_scope::for_each_service_in_scope_with_info;
use crate::{cli, config};

mod random_ports;
use random_ports::RandomPortsRecreateOptions;

pub(crate) struct HandleRecreateOptions<'a> {
    pub(crate) service: Option<&'a str>,
    pub(crate) services: &'a [String],
    pub(crate) kind: Option<config::Kind>,
    pub(crate) profile: Option<&'a str>,
    pub(crate) healthy: bool,
    pub(crate) timeout: Option<u64>,
    pub(crate) random_ports: bool,
    pub(crate) env_output: bool,
    pub(crate) parallel: usize,
    pub(crate) quiet: bool,
    pub(crate) runtime_env: Option<&'a str>,
    pub(crate) config_path: Option<&'a Path>,
    pub(crate) project_root: Option<&'a Path>,
}

pub(crate) fn handle_recreate(
    config: &mut config::Config,
    options: HandleRecreateOptions<'_>,
) -> Result<()> {
    // Refused before anything is recreated, `-P` or not.
    if options.env_output {
        anyhow::bail!("{}", cli::support::env_write_removed());
    }
    let runtime = cli::support::resolve_project_runtime_context(
        config,
        options.config_path,
        options.project_root,
    )?;
    let start_context = runtime.service_start_context();

    // Recreating removes the container first, so a host port it was given
    // by an earlier `up -P` is read before that.
    for service in &mut config.service {
        crate::network::inherit_publication(service);
    }
    if options.random_ports {
        for service in &mut config.service {
            service.runtime.publish_host_port = true;
        }
        return random_ports::handle_random_ports_recreate(
            config,
            RandomPortsRecreateOptions {
                service: options.service,
                services: options.services,
                kind: options.kind,
                profile: options.profile,
                healthy: options.healthy,
                timeout: options.timeout,
                env_output: options.env_output,
                quiet: options.quiet,
                runtime_env_name: options.runtime_env,
                config_path: options.config_path,
                project_root: options.project_root,
                parallel: options.parallel,
                workspace_root: &runtime.workspace_root,
            },
        );
    }

    for_each_service_in_scope_with_info(
        config,
        options.service,
        options.services,
        options.kind,
        options.profile,
        options.parallel,
        options.quiet,
        "Recreating service",
        |svc| cli::support::recreate_service(svc, &start_context, options.healthy, options.timeout),
    )?;

    Ok(())
}
