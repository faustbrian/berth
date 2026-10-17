//! cli handlers docker ops top module.
//!
//! Contains top handler used by Berth command workflows.

use anyhow::Result;

use crate::{config, docker};

pub(crate) fn handle_top(
    config: &config::Config,
    service: Option<&str>,
    services: &[String],
    kind: Option<config::Kind>,
    profile: Option<&str>,
    args: &[String],
) -> Result<()> {
    super::run_for_each_docker_service_in_scope(
        config,
        service,
        services,
        kind,
        profile,
        1,
        |svc| docker::top(svc, args),
    )
}
