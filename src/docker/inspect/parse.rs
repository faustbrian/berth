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
    // `NetworkSettings.Ports` holds the live binding but Docker clears it
    // when the container stops; `HostConfig.PortBindings` keeps the binding
    // the container was created with, so a stopped container still reads as
    // published and is started again rather than recreated.
    let live = entry
        .get("NetworkSettings")
        .and_then(|settings| settings.get("Ports"))
        .and_then(|ports| binding_of(ports, &key));
    let binding = live.or_else(|| {
        entry
            .get("HostConfig")
            .and_then(|config| config.get("PortBindings"))
            .and_then(|ports| binding_of(ports, &key))
    })?;
    let host = binding.get("HostIp")?.as_str()?.to_owned();
    let port = binding.get("HostPort")?.as_str()?.parse::<u16>().ok()?;
    Some((host, port))
}

fn binding_of<'a>(ports: &'a Value, key: &str) -> Option<&'a Value> {
    ports.get(key)?.as_array()?.first()
}

#[cfg(test)]
mod tests {
    use super::extract_host_port_binding_from_inspect;

    #[test]
    fn a_stopped_container_reads_its_configured_binding() {
        let stopped = r#"[{"State":{"Status":"exited"},"NetworkSettings":{"Ports":{}},
            "HostConfig":{"PortBindings":{"5432/tcp":[{"HostIp":"127.0.0.1","HostPort":"61234"}]}}}]"#;
        assert_eq!(
            extract_host_port_binding_from_inspect(stopped, 5432),
            Some(("127.0.0.1".to_owned(), 61234))
        );
        let unpublished = r#"[{"NetworkSettings":{"Ports":{}},"HostConfig":{"PortBindings":{}}}]"#;
        assert_eq!(
            extract_host_port_binding_from_inspect(unpublished, 5432),
            None
        );
        let running = r#"[{"NetworkSettings":{"Ports":{"5432/tcp":[{"HostIp":"127.0.0.1","HostPort":"61235"}]}},
            "HostConfig":{"PortBindings":{"5432/tcp":[{"HostIp":"127.0.0.1","HostPort":"61235"}]}}}]"#;
        assert_eq!(
            extract_host_port_binding_from_inspect(running, 5432),
            Some(("127.0.0.1".to_owned(), 61235))
        );
    }
}
