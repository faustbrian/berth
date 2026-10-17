//! The URL a host tool can use to reach a service.

use crate::config::ServiceConfig;

/// Hint for services without a host port.
pub(crate) const PUBLISH_HINT: &str =
    "not published on the host; set `publish: true` on the service or run `berth up -P`";

/// Returns the URL that reaches the service from the host: its gateway route,
/// or its published host port. `None` when it has neither, because it is
/// reachable only inside its instance network.
#[must_use]
pub(crate) fn host_url(service: &ServiceConfig) -> Option<String> {
    if let Some(route) = crate::gateway::route_urls(service).into_iter().next()
        && crate::gateway::has_gateway_routes(service)
    {
        return Some(route);
    }
    if !super::is_published(service) {
        return None;
    }
    // The real binding wins: `-P` assigns random ports the config never saw.
    let binding = service.container_name().ok().and_then(|name| {
        crate::docker::inspect_host_port_binding(&name, service.resolved_container_port())
    });
    let mut bound = service.clone();
    if let Some((_, port)) = binding {
        bound.port = port;
    }
    // An app serving TLS itself (`localhost_tls`) answers on `https://`.
    Some(
        if bound.kind == crate::config::Kind::App && bound.localhost_tls {
            crate::serve::local_url(&bound)
        } else {
            bound.connection_url()
        },
    )
}

#[cfg(test)]
mod tests {
    use super::host_url;
    use crate::config::ServiceConfig;

    fn service() -> ServiceConfig {
        serde_json::from_value(serde_json::json!({
            "name": "db", "kind": "database", "driver": "postgres",
            "image": "postgres:17", "host": "127.0.0.1", "port": 54321
        }))
        .expect("service fixture")
    }

    #[test]
    fn unpublished_instance_services_have_no_host_url() {
        let mut service = service();
        service.runtime.network = Some("berth_api_a-1".to_owned());
        assert_eq!(host_url(&service), None);
        service.runtime.publish_host_port = true;
        assert!(host_url(&service).expect("published").contains(":54321"));
    }

    #[test]
    fn a_published_app_serving_tls_is_reached_over_https() {
        let mut app: ServiceConfig = serde_json::from_value(serde_json::json!({
            "name": "app", "kind": "app", "driver": "frankenphp",
            "image": "dunglas/frankenphp:php8.5", "host": "127.0.0.1", "port": 57446,
            "container_port": 443, "localhost_tls": true
        }))
        .expect("app fixture");
        app.runtime.publish_host_port = true;
        assert_eq!(
            host_url(&app).expect("published"),
            "https://localhost:57446"
        );
    }

    #[test]
    fn routed_apps_are_reached_through_their_route() {
        let mut app: ServiceConfig = serde_json::from_value(serde_json::json!({
            "name": "app", "kind": "app", "driver": "frankenphp",
            "image": "dunglas/frankenphp", "host": "127.0.0.1", "port": 8000
        }))
        .expect("app fixture");
        app.runtime.network = Some("berth_api_a-1".to_owned());
        app.resolved_domain = Some("a-1-api.berth.localhost".to_owned());
        app.runtime.gateway_https_port = Some(8443);
        assert_eq!(
            host_url(&app).as_deref(),
            Some("https://a-1-api.berth.localhost:8443")
        );
    }
}
