//! Env inference rules for app-type targets.

use std::collections::HashMap;

use crate::config::{Driver, ServiceConfig};

use super::{
    inferred_app_public_url, insert_if_absent, is_app_driver, runtime_host_for_app,
    service_endpoint,
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
        insert_if_absent(vars, "APP_URL", url.clone());
        insert_if_absent(vars, "ASSET_URL", url);
        insert_if_absent(
            vars,
            "LIVEWIRE_TEMPORARY_FILE_UPLOAD_DISK",
            "local".to_owned(),
        );
    }

    if is_app_driver(service, Driver::Gotenberg) {
        insert_if_absent(vars, "GOTENBERG_BASE_URL", service_endpoint(service));
    }

    if is_app_driver(service, Driver::Mailhog) {
        let smtp_port = service
            .smtp_port
            .unwrap_or(service.port.saturating_add(1000));
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
        insert_if_absent(vars, "RABBITMQ_PORT", service.port.to_string());
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
