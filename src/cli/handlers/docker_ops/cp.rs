//! cli handlers docker ops cp module.
//!
//! Contains cp handler used by Berth command workflows.

use anyhow::Result;

use crate::{config, docker};

pub(crate) struct HandleCpOptions<'a> {
    pub(crate) source: &'a str,
    pub(crate) destination: &'a str,
    pub(crate) follow_link: bool,
    pub(crate) archive: bool,
}

pub(crate) fn handle_cp(config: &config::Config, options: HandleCpOptions<'_>) -> Result<()> {
    let source_resolved = resolve_cp_endpoint(config, options.source)?;
    let destination_resolved = resolve_cp_endpoint(config, options.destination)?;
    docker::cp(
        &source_resolved,
        &destination_resolved,
        docker::CpOptions {
            follow_link: options.follow_link,
            archive: options.archive,
        },
    )
}

fn resolve_cp_endpoint(config: &config::Config, endpoint: &str) -> Result<String> {
    let Some((prefix, path)) = endpoint.split_once(':') else {
        return Ok(endpoint.to_owned());
    };

    if prefix.is_empty() {
        return Ok(endpoint.to_owned());
    }

    let Ok(svc) = config::find_service(config, prefix) else {
        return Ok(endpoint.to_owned());
    };
    let container_name = svc.container_name()?;
    Ok(format!("{container_name}:{path}"))
}
