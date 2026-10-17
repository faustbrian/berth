//! Route host names: `{instance}-{project}[-{service}].berth.localhost`.

use anyhow::Result;

use crate::config::{Driver, Kind, ServiceConfig};

/// Suffix every berth route lives under.
pub(crate) const ROUTE_SUFFIX: &str = "berth.localhost";

/// Builds the route host for a service. The service named `app` is the
/// instance's primary route and has no service segment.
///
/// # Errors
///
/// Returns an error when the first label would exceed the 63-byte DNS limit,
/// naming the parts so the user can shorten one; berth never truncates.
pub(crate) fn route_host(instance: &str, project: &str, service: &str) -> Result<String> {
    let label = if service == "app" {
        format!("{instance}-{project}")
    } else {
        format!("{instance}-{project}-{service}")
    };
    if !label
        .bytes()
        .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
    {
        anyhow::bail!("route label '{label}' may contain only lowercase letters, digits and '-'");
    }
    if label.len() > 63 {
        anyhow::bail!(
            "route label '{label}' is {} bytes; DNS labels are limited to 63. Shorten the \
             instance ('{instance}'), project ('{project}') or service ('{service}') name",
            label.len()
        );
    }
    Ok(format!("{label}.{ROUTE_SUFFIX}"))
}

/// Whether the service is served through the gateway right now: it has its
/// instance route and is not a direct `localhost` TLS target (the test
/// runtime).
#[must_use]
pub(crate) fn has_gateway_routes(service: &ServiceConfig) -> bool {
    service.runtime.gateway_https_port.is_some()
        && !service.localhost_tls
        && service.resolved_domain.is_some()
}

/// The hosts the gateway serves for a service: its unique instance route
/// first, then its authored domains.
#[must_use]
pub(crate) fn route_hosts(service: &ServiceConfig) -> Vec<String> {
    if !has_gateway_routes(service) {
        return Vec::new();
    }
    let mut hosts: Vec<String> = service.resolved_domain.iter().cloned().collect();
    for domain in service
        .domain
        .iter()
        .chain(service.domains.iter().flatten())
    {
        let domain = domain.trim().to_ascii_lowercase();
        if !domain.is_empty() && !hosts.contains(&domain) {
            hosts.push(domain);
        }
    }
    hosts
}

/// The URLs a service's routes are served at, including the gateway's HTTPS
/// port, its unique instance route first; for services without gateway
/// routes, their authored domain URLs.
#[must_use]
pub(crate) fn route_urls(service: &ServiceConfig) -> Vec<String> {
    match service.runtime.gateway_https_port {
        Some(port) if has_gateway_routes(service) => route_hosts(service)
            .iter()
            .map(|host| super::ports::https_origin(host, port))
            .collect(),
        _ => service.resolved_domain_urls(),
    }
}

/// Whether `host` is a route of the instance (and runtime environment) the
/// service runs in. Those routes are network aliases of the gateway on the
/// instance network, so containers reach them without leaving it.
#[must_use]
pub(crate) fn is_instance_route(service: &ServiceConfig, host: &str) -> bool {
    let (Some(project), Some(network)) = (&service.runtime.project, &service.runtime.network)
    else {
        return false;
    };
    // `berth_{project}_{instance}[-{env}]` → `{instance}[-{env}]-{project}`.
    let Some(instance) = network.strip_prefix(&format!("berth_{project}_")) else {
        return false;
    };
    let Some(label) = host.strip_suffix(&format!(".{ROUTE_SUFFIX}")) else {
        return false;
    };
    let prefix = format!("{instance}-{project}");
    label == prefix || label.strip_prefix(&format!("{prefix}-")).is_some()
}

/// Whether a service serves HTTP and gets a gateway route: web apps, the
/// mail UI and WebSocket servers. Workers, schedulers, Horizon, RabbitMQ,
/// Gotenberg and Selenium are reached only inside the instance network.
#[must_use]
pub(crate) fn is_routable(service: &ServiceConfig) -> bool {
    if service.kind != Kind::App {
        return false;
    }
    match service.driver {
        // A FrankenPHP service with its own command and no health path is a
        // worker (for example the `queue-worker` preset), not a web server.
        Driver::Frankenphp => service.command.is_none() || service.health_path.is_some(),
        Driver::Mailhog | Driver::Reverb | Driver::Soketi => true,
        _ => false,
    }
}

/// Validates a domain authored in `.berth.yaml`: the gateway's wildcard
/// certificate and `.localhost` loopback resolution cover exactly one label
/// directly under `berth.localhost`, so any other name could neither resolve
/// nor be served without editing `/etc/hosts`.
///
/// # Errors
///
/// Returns an error naming the domain and the accepted shape.
pub(crate) fn validate_authored_domain(service: &str, domain: &str) -> Result<()> {
    let label = domain.strip_suffix(&format!(".{ROUTE_SUFFIX}"));
    let valid = label.is_some_and(|label| {
        !label.is_empty()
            && label.len() <= 63
            && !label.starts_with('-')
            && !label.ends_with('-')
            && label
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
    });
    if !valid {
        anyhow::bail!(
            "service '{service}' domain '{domain}' must be one lowercase label under \
             {ROUTE_SUFFIX} (for example 'shop.{ROUTE_SUFFIX}')"
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{route_host, validate_authored_domain};

    #[test]
    fn authored_domains_must_be_one_label_under_the_route_suffix() {
        validate_authored_domain("app", "shop.berth.localhost").expect("valid");
        for invalid in [
            "shop.test",
            "a.b.berth.localhost",
            "Shop.berth.localhost",
            "berth.localhost",
            "-a.berth.localhost",
        ] {
            assert!(
                validate_authored_domain("app", invalid).is_err(),
                "{invalid}"
            );
        }
    }

    #[test]
    fn primary_app_routes_omit_the_service_segment() {
        assert_eq!(
            route_host("agent-1-3fa29c", "shipit-api", "app").expect("route"),
            "agent-1-3fa29c-shipit-api.berth.localhost"
        );
        assert_eq!(
            route_host("agent-1-3fa29c", "shipit-api", "mailpit").expect("route"),
            "agent-1-3fa29c-shipit-api-mailpit.berth.localhost"
        );
    }

    #[test]
    fn overlong_labels_fail_instead_of_truncating() {
        let instance = "a".repeat(23);
        let project = "b".repeat(24);
        assert!(
            route_host(&instance, &project, &"c".repeat(14)).is_ok(),
            "exactly 63 bytes"
        );
        let error = route_host(&instance, &project, &"c".repeat(15)).expect_err("64 bytes");
        assert!(error.to_string().contains("63"), "{error}");
    }
}
