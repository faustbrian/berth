//! doctor app repair checks.

use crate::cli::support::run_doctor::report;
use crate::{cli, config, docker, serve};

const REPAIR_TIMEOUT_SECS: u64 = 30;
const REPAIR_INTERVAL_SECS: u64 = 2;

/// Repairs an unhealthy running HTTP app runtime when `doctor --fix` is enabled.
pub(in crate::cli::support::run_doctor::app) fn repair_unhealthy_http_runtime(
    target: &config::ServiceConfig,
    fix: bool,
) -> bool {
    if !fix || !is_running(target) {
        return false;
    }

    let Some(app_url) = resolve_app_url(target) else {
        return false;
    };
    let health_url = cli::support::build_health_url(target, &app_url, None);

    if endpoint_healthy(target, &app_url, true) && endpoint_healthy(target, &health_url, false) {
        return false;
    }

    report::info(&format!(
        "App service '{}' appears unhealthy; attempting restart repair",
        target.name
    ));
    if let Err(err) = docker::restart(target) {
        report::error(&format!(
            "App service '{}' restart repair failed: {err}",
            target.name
        ));
        return true;
    }
    if let Err(err) = serve::wait_until_http_healthy(
        target,
        Some(REPAIR_TIMEOUT_SECS),
        REPAIR_INTERVAL_SECS,
        None,
    ) {
        report::error(&format!(
            "App service '{}' did not recover after restart: {err}",
            target.name
        ));
        return true;
    }

    report::success(&format!(
        "App service '{}' recovered after restart",
        target.name
    ));
    false
}

fn is_running(target: &config::ServiceConfig) -> bool {
    target
        .container_name()
        .ok()
        .and_then(|name| docker::inspect_status(&name))
        .is_some_and(|status| status == "running")
}

fn resolve_app_url(target: &config::ServiceConfig) -> Option<String> {
    match serve::public_url(target) {
        Ok(url) => Some(url),
        Err(err) => {
            report::info(&format!(
                "Skipping runtime repair probe for '{}' (URL unavailable: {err})",
                target.name
            ));
            None
        }
    }
}

fn endpoint_healthy(target: &config::ServiceConfig, url: &str, app_url: bool) -> bool {
    let Some(status_code) = cli::support::probe_http_status(url) else {
        return false;
    };

    if app_url {
        return (200..400).contains(&status_code);
    }

    cli::support::health_status_accepted(target, status_code)
}
