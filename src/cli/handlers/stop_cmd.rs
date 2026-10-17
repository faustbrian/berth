//! cli handlers stop cmd module.
//!
//! Contains cli handlers stop cmd logic used by Berth command workflows.

use anyhow::Result;

use super::service_scope::for_each_service_in_scope_with_info;
use crate::{config, docker};

pub(crate) fn handle_stop(
    config: &config::Config,
    service: Option<&str>,
    services: &[String],
    kind: Option<config::Kind>,
    profile: Option<&str>,
    timeout: u64,
    parallel: usize,
    quiet: bool,
) -> Result<()> {
    for_each_service_in_scope_with_info(
        config,
        service,
        services,
        kind,
        profile,
        parallel,
        quiet,
        "Stopping service",
        |svc| docker::stop(svc, timeout),
    )
}
