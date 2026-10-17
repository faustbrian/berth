//! docker inspect tests module.
//!
//! Contains docker inspect tests logic used by Berth command workflows.

use super::parse::extract_host_port_binding_from_inspect;

#[test]
fn extracts_runtime_binding_from_docker_inspect_json() {
    let payload = r#"[
            {
                "NetworkSettings": {
                    "Ports": {
                        "3306/tcp": [
                            { "HostIp": "127.0.0.1", "HostPort": "49123" }
                        ]
                    }
                }
            }
        ]"#;

    let binding = extract_host_port_binding_from_inspect(payload, 3306);
    assert_eq!(binding, Some(("127.0.0.1".to_owned(), 49123)));
}

#[test]
fn returns_none_when_binding_missing() {
    let payload = r#"[
            {
                "NetworkSettings": {
                    "Ports": {
                        "6379/tcp": null
                    }
                }
            }
        ]"#;

    let binding = extract_host_port_binding_from_inspect(payload, 3306);
    assert_eq!(binding, None);
}
