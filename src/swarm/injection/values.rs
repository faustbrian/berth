//! swarm injection values module.
//!
//! Resolves `inject_env` tokens to endpoints a consumer's container can
//! call. Route URLs (`https://….berth.localhost`) work from a browser but not
//! from inside a container: libcurl (PHP's curl, Guzzle, Laravel `Http::`)
//! resolves every `*.localhost` name to loopback itself (curl 7.85+), so
//! `:url`, `:host` and `:port` name the source app container on its instance
//! network, which the consumer joins (ledger L205).

use anyhow::Result;

/// Resolves injected env value using configured inputs and runtime state.
///
/// # Errors
///
/// Returns an error for an unknown token, or `:domain` on a source without
/// a route.
pub(crate) fn resolve_injected_env_value(
    value_spec: &str,
    source_service: &crate::config::ServiceConfig,
) -> Result<String> {
    if !value_spec.starts_with(':') {
        return Ok(value_spec.to_owned());
    }
    let container = || source_service.container_name();
    let port = source_service.resolved_container_port();

    match value_spec {
        ":domain" => {
            let routes = crate::gateway::route_hosts(source_service);
            let Some(domain) = routes
                .first()
                .map(String::as_str)
                .or(source_service.primary_domain())
            else {
                anyhow::bail!(
                    "requested ':domain' but source service '{}' has no domain",
                    source_service.name
                );
            };
            Ok(domain.to_owned())
        }
        ":host" => container(),
        ":port" => Ok(port.to_string()),
        // Inside the network the app serves plain HTTP; TLS ends at the
        // gateway.
        ":scheme" => Ok("http".to_owned()),
        ":base_url" | ":url" => Ok(format!("http://{}:{port}", container()?)),
        _ => anyhow::bail!("unsupported injected env token '{value_spec}'"),
    }
}
