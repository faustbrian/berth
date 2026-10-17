//! docker inspect parse module.
//!
//! Contains docker inspect parse logic used by Berth command workflows.

use serde_json::Value;

pub(super) fn extract_host_port_binding_from_inspect(
    payload: &str,
    container_port: u16,
) -> Option<(String, u16)> {
    let inspect: Value = serde_json::from_str(payload).ok()?;
    let entry = inspect.as_array()?.first()?;
    let key = format!("{container_port}/tcp");
    let binding = entry
        .get("NetworkSettings")?
        .get("Ports")?
        .get(&key)?
        .as_array()?
        .first()?;
    let host = binding.get("HostIp")?.as_str()?.to_owned();
    let port = binding.get("HostPort")?.as_str()?.parse::<u16>().ok()?;
    Some((host, port))
}
