//! cli handlers docker ops unpause module.
//!
//! Contains unpause handler used by Berth command workflows.

use anyhow::Result;

use crate::{config, docker};

pub(crate) fn handle_unpause(
    config: &config::Config,
    service: Option<&str>,
    services: &[String],
    kind: Option<config::Kind>,
    profile: Option<&str>,
    parallel: usize,
) -> Result<()> {
    super::run_for_each_docker_service_in_scope(
        config,
        service,
        services,
        kind,
        profile,
        parallel,
        docker::unpause,
    )
}
