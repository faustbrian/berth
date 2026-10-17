//! Doctor port check helpers.

use std::collections::HashMap;

use crate::cli;
use crate::cli::support::run_doctor::report;
use crate::config;
use crate::docker;

pub(super) fn service_is_running(service: &config::ServiceConfig) -> bool {
    let Ok(container_name) = service.container_name() else {
        return false;
    };

    docker::inspect_status(&container_name).as_deref() == Some("running")
}

pub(super) fn check_service_ports<'a>(
    service: &'a config::ServiceConfig,
    running: bool,
    used_ports: &mut HashMap<(String, u16), &'a str>,
) -> bool {
    let mut has_error = false;
    // A service without a host port never binds one.
    if !crate::network::is_published(service) {
        return false;
    }

    for (host, port) in cli::support::service_host_ports(service) {
        if let Some(existing) = used_ports.insert((host.clone(), port), service.name.as_str()) {
            if existing != service.name {
                has_error = true;
                emit_doctor_error(&format!(
                    "Port conflict: {}:{} used by '{}' and '{}'",
                    host, port, existing, service.name
                ));
            }
        }
        if !running && !cli::support::is_port_available_strict(&host, port) {
            has_error = true;
            emit_doctor_error(&format!(
                "Host port unavailable: {}:{} is already in use for '{}'",
                host, port, service.name
            ));
        }
    }

    has_error
}

fn emit_doctor_error(message: &str) {
    report::error(message);
}
