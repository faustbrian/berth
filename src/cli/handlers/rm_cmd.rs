//! cli handlers rm cmd module.
//!
//! Contains cli handlers rm cmd logic used by Berth command workflows.

use anyhow::Result;

use super::service_scope::for_each_service_in_scope_with_info;
use crate::{config, docker};

pub(crate) struct HandleRmOptions<'a> {
    pub(crate) service: Option<&'a str>,
    pub(crate) services: &'a [String],
    pub(crate) kind: Option<config::Kind>,
    pub(crate) profile: Option<&'a str>,
    pub(crate) force: bool,
    pub(crate) parallel: usize,
    pub(crate) quiet: bool,
}

pub(crate) fn handle_rm(config: &config::Config, options: HandleRmOptions<'_>) -> Result<()> {
    for_each_service_in_scope_with_info(
        config,
        options.service,
        options.services,
        options.kind,
        options.profile,
        options.parallel,
        options.quiet,
        "Removing service container",
        |svc| docker::rm(svc, options.force),
    )
}
