//! cli handlers docker ops attach module.
//!
//! Contains attach handler used by Berth command workflows.

use anyhow::Result;

use crate::{config, docker};

pub(crate) struct HandleAttachOptions<'a> {
    pub(crate) service: Option<&'a str>,
    pub(crate) no_stdin: bool,
    pub(crate) sig_proxy: bool,
    pub(crate) detach_keys: Option<&'a str>,
}

pub(crate) fn handle_attach(
    config: &config::Config,
    options: HandleAttachOptions<'_>,
) -> Result<()> {
    let svc = config::resolve_service(config, options.service)?;
    docker::attach(
        svc,
        options.no_stdin,
        options.sig_proxy,
        options.detach_keys,
    )
}
