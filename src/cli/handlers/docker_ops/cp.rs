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

/// Maps `service:/path` to the service's container. As Docker does, an
/// argument starting with `/` or `.` is a host path, and so is one without
/// `:`. Any other `name:/path` must name one of this instance's services or
/// their containers: a raw name could reach another instance's container or
/// a shared server, whose files are every instance's data.
fn resolve_cp_endpoint(config: &config::Config, endpoint: &str) -> Result<String> {
    if endpoint.starts_with('/') || endpoint.starts_with('.') {
        return Ok(endpoint.to_owned());
    }
    let Some((prefix, path)) = endpoint.split_once(':') else {
        return Ok(endpoint.to_owned());
    };
    if prefix.is_empty() {
        return Ok(endpoint.to_owned());
    }

    let service = match config::find_service(config, prefix) {
        Ok(service) => service,
        Err(_) => config
            .service
            .iter()
            .find(|service| service.container_name().is_ok_and(|name| name == prefix))
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "`{prefix}` is not a service of this instance; berth cp copies only to \
                     and from this instance's services (use the service name)"
                )
            })?,
    };
    // A shared server's files are every instance's data.
    crate::shared::refuse_server_operation(service, "copy files in or out of")?;
    let container_name = service.container_name()?;
    Ok(format!("{container_name}:{path}"))
}
