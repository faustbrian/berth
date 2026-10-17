//! cli handlers docker ops pause module.
//!
//! Contains pause handler used by Berth command workflows.

use anyhow::Result;

use crate::{config, docker};

pub(crate) fn handle_pause(
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
        |svc| {
            // A shared tenant is refused only when named: pausing the whole
            // instance leaves the server other instances use alone.
            if svc.runtime.shared.is_some() && service.is_none() && services.is_empty() {
                return Ok(());
            }
            docker::pause(svc)
        },
    )
}
