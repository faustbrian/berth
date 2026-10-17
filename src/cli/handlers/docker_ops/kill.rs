//! cli handlers docker ops kill module.
//!
//! Contains kill handler used by Berth command workflows.

use anyhow::Result;

use crate::{config, docker};

pub(crate) fn handle_kill(
    config: &config::Config,
    service: Option<&str>,
    services: &[String],
    kind: Option<config::Kind>,
    profile: Option<&str>,
    signal: Option<&str>,
    parallel: usize,
) -> Result<()> {
    super::run_for_each_docker_service_in_scope(
        config,
        service,
        services,
        kind,
        profile,
        parallel,
        |svc| docker::kill(svc, signal),
    )
}
