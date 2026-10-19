//! Resolves and validates the extra per-instance routes of a service.

use anyhow::Result;

use crate::config::{Config, ExtraRoute, ServiceConfig};

/// Env vars berth itself sets from the instance route; a route may not
/// replace them.
const RESERVED_ENV: [&str; 2] = ["APP_URL", "ASSET_URL"];

/// Resolves a routable service's `routes` against its instance route
/// `primary`, failing on the first invalid label or env var.
///
/// # Errors
///
/// Returns an error naming the service and the offending label or env var.
pub(super) fn resolve_extra_routes(
    service: &ServiceConfig,
    primary: &str,
) -> Result<Vec<ExtraRoute>> {
    let Some(routes) = &service.routes else {
        return Ok(Vec::new());
    };
    let name = &service.name;
    if routes.is_empty() {
        anyhow::bail!("service '{name}' `routes` is empty; list at least one route or remove it");
    }
    let mut resolved: Vec<ExtraRoute> = Vec::with_capacity(routes.len());
    for (label, env) in routes {
        if !crate::config::project_file::schema::is_dns_label(label) {
            anyhow::bail!(
                "service '{name}' route '{label}' must be lowercase letters, digits and inner \
                 hyphens"
            );
        }
        crate::config::project_file::validate_env_key(name, env)?;
        if RESERVED_ENV.contains(&env.as_str()) {
            anyhow::bail!(
                "service '{name}' route '{label}' sets {env}, which berth sets to the instance route"
            );
        }
        if service
            .env
            .as_ref()
            .is_some_and(|vars| vars.contains_key(env))
        {
            anyhow::bail!(
                "service '{name}' route '{label}' sets {env}, which its `env` also sets; remove \
                 one of them"
            );
        }
        if let Some(other) = resolved.iter().find(|route| &route.env == env) {
            anyhow::bail!(
                "service '{name}' routes '{}' and '{label}' both set {env}",
                other.label
            );
        }
        let host = crate::gateway::extra_route_host(primary, label)
            .map_err(|error| anyhow::anyhow!("service '{name}': {error}"))?;
        resolved.push(ExtraRoute {
            label: label.clone(),
            env: env.clone(),
            host,
        });
    }
    Ok(resolved)
}

/// Fails when an extra route's host is also another route of the instance:
/// a service's own route, another extra route or an authored domain; and
/// when two services' routes set the same env var.
///
/// # Errors
///
/// Returns an error naming the host and every service claiming it.
pub(super) fn ensure_extra_routes_are_unique(services: &[ServiceConfig]) -> Result<()> {
    let claims: Vec<(&str, String)> = services
        .iter()
        .flat_map(|service| {
            let authored = service
                .domain
                .iter()
                .chain(service.domains.iter().flatten())
                .map(|domain| domain.trim().to_ascii_lowercase());
            service
                .resolved_domain
                .iter()
                .cloned()
                .chain(
                    service
                        .runtime
                        .extra_routes
                        .iter()
                        .map(|route| route.host.clone()),
                )
                .chain(authored)
                .map(move |host| (service.name.as_str(), host))
        })
        .collect();
    let mut vars: Vec<(&str, &ExtraRoute)> = Vec::new();
    for service in services {
        for route in &service.runtime.extra_routes {
            if let Some((other, earlier)) =
                vars.iter().find(|(_, earlier)| earlier.env == route.env)
            {
                // The env is shared by every app container, so one var can
                // hold only one route's URL.
                anyhow::bail!(
                    "service '{}' route '{}' and service '{other}' route '{}' both set {}",
                    service.name,
                    route.label,
                    earlier.label,
                    route.env
                );
            }
            vars.push((service.name.as_str(), route));
            let claimants: Vec<String> = claims
                .iter()
                .filter(|(_, host)| *host == route.host)
                .map(|(name, _)| format!("service '{name}'"))
                .collect();
            if claimants.len() > 1 {
                anyhow::bail!(
                    "service '{}' route '{}' is {}, which is also claimed by {}",
                    service.name,
                    route.label,
                    route.host,
                    claimants.join(" and ")
                );
            }
        }
    }
    Ok(())
}

/// Env vars berth's managed runtime sets for Laravel apps on top of the
/// inferred values (`env::apply_managed_runtime_env`) and FrankenPHP's
/// listener.
const MANAGED_ENV: [&str; 3] = ["APP_KEY", "APP_CONFIG_CACHE", "SERVER_NAME"];

/// Fails when a route's env var is one berth already sets: an inferred
/// value (service endpoints, `APP_NAME`, …) or a managed one. Those are
/// inserted before the route's URL and keep their value, so the URL would
/// otherwise be dropped without a word. Swarm `inject_env` vars come from
/// the workspace that starts the project and are refused where they are
/// merged (`cli::support::runtime_app_env`).
///
/// # Errors
///
/// Returns an error naming the service, route and var.
pub(super) fn ensure_route_vars_are_free(config: &Config) -> Result<()> {
    if config
        .service
        .iter()
        .all(|service| service.routes.is_none())
    {
        return Ok(());
    }
    let mut without_routes = config.clone();
    for service in &mut without_routes.service {
        service.routes = None;
        service.runtime.extra_routes.clear();
    }
    let inferred = crate::env::inferred_app_env(&without_routes);
    for service in &config.service {
        for (label, env) in service.routes.iter().flatten() {
            if inferred.contains_key(env) || MANAGED_ENV.contains(&env.as_str()) {
                anyhow::bail!(
                    "service '{}' route '{label}' sets {env}, which berth already sets for the \
                     app",
                    service.name
                );
            }
        }
    }
    Ok(())
}
