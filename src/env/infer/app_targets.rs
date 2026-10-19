//! Env inference rules for app-type targets.

use std::collections::HashMap;

use crate::config::{Driver, ServiceConfig};

use super::{
    inferred_app_public_url, insert_if_absent, is_app_driver, runtime_host_for_app,
    runtime_port_for_app, service_endpoint,
};

mod dusk;
mod horizon;
mod reverb;
mod scheduler;
mod soketi;

/// Applies app-specific inferred env variables.
///
/// Variables are only inserted when absent so explicit user env settings keep
/// precedence.
pub(super) fn apply_app_target_env(vars: &mut HashMap<String, String>, service: &ServiceConfig) {
    // Some app stacks (e.g. JSON-RPC server metadata) require APP_NAME to be
    // present explicitly and do not fall back to config('app.name').
    insert_if_absent(vars, "APP_NAME", "Laravel".to_owned());

    if is_app_driver(service, Driver::Frankenphp)
        && let Some(url) = inferred_app_public_url(service)
    {
        // A site served on an extra route would load its assets from the
        // primary origin, and browsers refuse cross-origin module scripts
        // without CORS headers. Without `ASSET_URL`, Laravel's `asset()`
        // follows the host each request came in on.
        // `ASSET_URL` belongs to the app that sets `APP_URL`, never to a
        // later web app of the project.
        if !vars.contains_key("APP_URL") && service.routes.is_none() {
            insert_if_absent(vars, "ASSET_URL", url.clone());
        }
        insert_if_absent(vars, "APP_URL", url);
        insert_if_absent(
            vars,
            "LIVEWIRE_TEMPORARY_FILE_UPLOAD_DISK",
            "local".to_owned(),
        );
    }

    apply_route_env(vars, service);

    if is_app_driver(service, Driver::Gotenberg) {
        insert_if_absent(vars, "GOTENBERG_BASE_URL", service_endpoint(service));
    }

    if is_app_driver(service, Driver::Mailhog) {
        let smtp_port = service
            .smtp_port
            .unwrap_or(service.port.saturating_add(1000));
        // Inside an instance network the SMTP listener is the container's.
        let smtp_port = if service.runtime.network.is_some() {
            1025
        } else {
            smtp_port
        };
        insert_if_absent(vars, "MAIL_MAILER", "smtp".to_owned());
        insert_if_absent(vars, "MAIL_HOST", runtime_host_for_app(service));
        insert_if_absent(vars, "MAIL_PORT", smtp_port.to_string());
        insert_if_absent(vars, "MAIL_ENCRYPTION", "null".to_owned());
        insert_if_absent(vars, "MAIL_USERNAME", String::new());
        insert_if_absent(vars, "MAIL_PASSWORD", String::new());
        insert_if_absent(vars, "MAIL_FROM_ADDRESS", "hello@example.com".to_owned());
        insert_if_absent(vars, "MAIL_FROM_NAME", "Berth".to_owned());
    }

    if is_app_driver(service, Driver::Rabbitmq) {
        insert_if_absent(vars, "RABBITMQ_HOST", runtime_host_for_app(service));
        insert_if_absent(
            vars,
            "RABBITMQ_PORT",
            runtime_port_for_app(service).to_string(),
        );
        insert_if_absent(
            vars,
            "RABBITMQ_USER",
            service
                .username
                .clone()
                .unwrap_or_else(|| "guest".to_owned()),
        );
        insert_if_absent(
            vars,
            "RABBITMQ_PASSWORD",
            service
                .password
                .clone()
                .unwrap_or_else(|| "guest".to_owned()),
        );
    }

    if is_app_driver(service, Driver::Reverb) {
        reverb::apply(vars, service);
    }

    if is_app_driver(service, Driver::Soketi) {
        soketi::apply(vars, service);
    }

    if is_app_driver(service, Driver::Horizon) {
        horizon::apply(vars);
    }

    if is_app_driver(service, Driver::Scheduler) {
        scheduler::apply(vars);
    }

    if is_app_driver(service, Driver::Dusk) {
        dusk::apply(vars, service);
    }
}

/// Gives each of the service's `routes` env vars its route's URL. Without
/// gateway routes (`--env` namespaces and test leases) the extra hosts are
/// not served, and the vars are left to the app's own env files (its
/// `.env.testing`, say) rather than all set to one URL.
fn apply_route_env(vars: &mut HashMap<String, String>, service: &ServiceConfig) {
    let Some(port) = service.runtime.gateway_https_port else {
        return;
    };
    for route in crate::gateway::extra_routes(service) {
        insert_if_absent(
            vars,
            &route.env,
            crate::gateway::https_origin(&route.host, port),
        );
    }
}
