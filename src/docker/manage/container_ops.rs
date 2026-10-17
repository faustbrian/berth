//! docker manage container ops module.
//!
//! Contains docker manage container ops logic used by Berth command workflows.

use anyhow::Result;

use crate::config::ServiceConfig;

mod docker_cmd;
mod down;
mod recreate;
mod restart;
mod rm;
mod stop;

/// Downs down as part of the docker manage container ops workflow.
pub(super) fn down(service: &ServiceConfig, timeout: u64) -> Result<()> {
    down::down(service, timeout)
}

/// Stops stop as part of the docker manage container ops workflow.
pub(super) fn stop(service: &ServiceConfig, timeout: u64) -> Result<()> {
    stop::stop(service, timeout)
}

/// Rms rm as part of the docker manage container ops workflow.
pub(super) fn rm(service: &ServiceConfig, force: bool) -> Result<()> {
    rm::rm(service, force)
}

pub(super) fn recreate(service: &ServiceConfig) -> Result<()> {
    recreate::recreate(service)
}

/// Restarts restart as part of the docker manage container ops workflow.
pub(super) fn restart(service: &ServiceConfig) -> Result<()> {
    restart::restart(service)
}
