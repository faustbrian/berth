//! swarm injection values module.
//!
//! Contains swarm injection values logic used by Berth command workflows.

use anyhow::Result;

/// Resolves injected env value using configured inputs and runtime state.
pub(crate) fn resolve_injected_env_value(
    value_spec: &str,
    source_service: &crate::config::ServiceConfig,
    runtime_host_port: Option<u16>,
) -> Result<String> {
    if !value_spec.starts_with(':') {
        return Ok(value_spec.to_owned());
    }

    let effective_port = runtime_host_port.unwrap_or(source_service.port);

    match value_spec {
        ":domain" => {
            let Some(domain) = source_service.primary_domain() else {
                anyhow::bail!(
                    "requested ':domain' but source service '{}' has no domain",
                    source_service.name
                );
            };
            Ok(domain.to_owned())
        }
        ":host" => Ok(runtime_host_for_app(source_service)),
        ":port" => Ok(effective_port.to_string()),
        ":scheme" => Ok(source_service.scheme().to_owned()),
        ":base_url" | ":url" => {
            if let Some(domain) = source_service.primary_domain() {
                return Ok(format!("https://{domain}"));
            }
            Ok(format!(
                "{}://{}:{}",
                source_service.scheme(),
                runtime_host_for_app(source_service),
                effective_port
            ))
        }
        _ => anyhow::bail!("unsupported injected env token '{value_spec}'"),
    }
}

/// Resolves the host value an app container can actually reach at runtime.
///
/// Why: `localhost` inside a container points to itself, not the host machine.
fn runtime_host_for_app(service: &crate::config::ServiceConfig) -> String {
    if service.uses_host_gateway_alias() {
        return crate::docker::host_gateway_alias().to_owned();
    }
    service.host.clone()
}
