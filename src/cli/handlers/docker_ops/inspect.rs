//! cli handlers docker ops inspect module.
//!
//! Contains inspect handler used by Berth command workflows.

use anyhow::Result;

use crate::{config, docker};

use super::output_json::{collect_service_json, print_pretty_json};

pub(crate) struct HandleInspectOptions<'a> {
    pub(crate) service: Option<&'a str>,
    pub(crate) services: &'a [String],
    pub(crate) kind: Option<config::Kind>,
    pub(crate) profile: Option<&'a str>,
    pub(crate) format: Option<&'a str>,
    pub(crate) output: &'a str,
    pub(crate) json: bool,
    pub(crate) size: bool,
    pub(crate) object_type: Option<&'a str>,
}

pub(crate) fn handle_inspect(
    config: &config::Config,
    options: HandleInspectOptions<'_>,
) -> Result<()> {
    let selected = super::selected_docker_services_in_scope(
        config,
        options.service,
        options.services,
        options.kind,
        options.profile,
    )?;
    let output_json = options.json || options.output.eq_ignore_ascii_case("json");
    if output_json {
        let items = collect_service_json(selected, |service| {
            let container_name = service.container_name()?;
            let payload = docker::inspect_json(&container_name)
                .ok_or_else(|| anyhow::anyhow!("Failed to inspect container '{container_name}'"))?;
            Ok(vec![payload])
        })?;
        return print_pretty_json(&items);
    }

    for service in selected {
        docker::inspect_container(service, options.format, options.size, options.object_type)?;
    }
    Ok(())
}
