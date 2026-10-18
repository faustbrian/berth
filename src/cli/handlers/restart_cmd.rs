//! cli handlers restart cmd module.
//!
//! Contains cli handlers restart cmd logic used by Berth command workflows.

use anyhow::Result;

use super::service_scope::for_each_service_in_scope_with_info;
use crate::{config, docker};

pub(crate) struct HandleRestartOptions<'a> {
    pub(crate) service: Option<&'a str>,
    pub(crate) services: &'a [String],
    pub(crate) kind: Option<config::Kind>,
    pub(crate) profile: Option<&'a str>,
    pub(crate) healthy: bool,
    pub(crate) timeout: Option<u64>,
    pub(crate) parallel: usize,
}

pub(crate) fn handle_restart(
    config: &config::Config,
    options: HandleRestartOptions<'_>,
) -> Result<()> {
    for_each_service_in_scope_with_info(
        config,
        options.service,
        options.services,
        options.kind,
        options.profile,
        options.parallel,
        false,
        "Restarting service",
        |svc| {
            docker::restart(svc)?;
            if options.healthy {
                docker::wait_until_healthy(svc, options.timeout, 2, None)?;
            }
            Ok(())
        },
    )
}
