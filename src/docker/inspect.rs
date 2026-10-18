//! docker inspect module.
//!
//! Contains docker inspect logic used by Berth command workflows.

use std::process::Output;

use super::is_dry_run;
use command::{docker_inspect, docker_inspect_format};
use label::inspect_label as inspect_label_value;
use parse::extract_host_port_binding_from_inspect;
pub(crate) use state::{ContainerState, container_state};

mod command;
mod label;
mod parse;
mod state;
#[cfg(test)]
mod tests;

#[must_use]
pub fn inspect_status(container_name: &str) -> Option<String> {
    if is_dry_run() {
        return Some("dry-run".to_owned());
    }

    let stdout = successful_stdout(docker_inspect_format(container_name, "{{.State.Status}}")?)?;
    Some(stdout.trim().to_owned())
}

/// The status of the container backing `service`: its own container, or for
/// a shared-backed service the shared server as the instance sees it.
#[must_use]
pub fn inspect_service_status(service: &crate::config::ServiceConfig) -> Option<String> {
    if service.runtime.shared.is_some() {
        return crate::shared::status(service);
    }
    inspect_status(&service.container_name().ok()?)
}

#[must_use]
pub fn inspect_host_port_binding(
    container_name: &str,
    container_port: u16,
) -> Option<(String, u16)> {
    if is_dry_run() {
        return None;
    }

    let payload = successful_stdout(docker_inspect(container_name)?)?;
    extract_host_port_binding_from_inspect(&payload, container_port)
}

#[must_use]
pub fn inspect_label(container_name: &str, key: &str) -> Option<String> {
    inspect_label_value(container_name, key)
}

#[must_use]
pub fn inspect_json(container_name: &str) -> Option<serde_json::Value> {
    if is_dry_run() {
        return Some(serde_json::json!({}));
    }

    let payload = successful_stdout(docker_inspect(container_name)?)?;
    let parsed: serde_json::Value = serde_json::from_str(&payload).ok()?;
    parsed.as_array()?.first().cloned()
}

fn successful_stdout(output: Output) -> Option<String> {
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}
