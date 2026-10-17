//! docker inspect module.
//!
//! Contains docker inspect logic used by Berth command workflows.

use std::collections::HashMap;
use std::process::Output;

use super::is_dry_run;
use command::{docker_inspect, docker_inspect_format};
use label::inspect_label as inspect_label_value;
use parse::extract_host_port_binding_from_inspect;

mod command;
mod label;
mod parse;
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

#[must_use]
pub fn inspect_env(container_name: &str) -> Option<HashMap<String, String>> {
    if is_dry_run() {
        return Some(HashMap::new());
    }

    let stdout = successful_stdout(docker_inspect_format(
        container_name,
        "{{range .Config.Env}}{{println .}}{{end}}",
    )?)?;

    let mut vars = HashMap::new();
    for line in stdout.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        vars.insert(key.to_owned(), value.to_owned());
    }
    Some(vars)
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
