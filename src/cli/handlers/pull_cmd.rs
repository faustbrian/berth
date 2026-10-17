//! cli handlers pull cmd module.
//!
//! Contains cli handlers pull cmd logic used by Berth command workflows.

use anyhow::Result;

use super::service_scope::for_each_service_in_scope;
use crate::{config, docker};

pub(crate) fn handle_pull(
    config: &config::Config,
    service: Option<&str>,
    services: &[String],
    kind: Option<config::Kind>,
    profile: Option<&str>,
    parallel: usize,
) -> Result<()> {
    for_each_service_in_scope(
        config,
        service,
        services,
        kind,
        profile,
        parallel,
        docker::pull,
    )
}
