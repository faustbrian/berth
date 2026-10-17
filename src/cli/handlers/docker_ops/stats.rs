//! cli handlers docker ops stats module.
//!
//! Contains stats handler used by Berth command workflows.

use anyhow::Result;

use crate::{config, docker};

pub(crate) fn handle_stats(
    config: &config::Config,
    service: Option<&str>,
    services: &[String],
    kind: Option<config::Kind>,
    profile: Option<&str>,
    no_stream: bool,
    format: Option<&str>,
) -> Result<()> {
    super::run_for_each_docker_service_in_scope(
        config,
        service,
        services,
        kind,
        profile,
        1,
        |svc| docker::stats(svc, docker::StatsOptions { no_stream, format }),
    )
}
