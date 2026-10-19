//! Driver-specific health response body validation.

use serde_json::Value;

use crate::config::ServiceConfig;

/// Returns whether response body content satisfies driver-specific health rules.
pub(super) fn body_health_is_ok(target: &ServiceConfig, body: &str) -> bool {
    if target.driver != crate::config::Driver::Gotenberg {
        return true;
    }

    let parsed: Value = match serde_json::from_str(body) {
        Ok(value) => value,
        Err(_) => return false,
    };
    parsed
        .get("status")
        .and_then(Value::as_str)
        .is_some_and(|status| status.eq_ignore_ascii_case("up"))
}

#[cfg(test)]
mod tests {
    use super::body_health_is_ok;
    use crate::config::{Driver, Kind, ServiceConfig};

    fn make_service(driver: Driver) -> ServiceConfig {
        ServiceConfig {
            name: "api".to_owned(),
            kind: Kind::App,
            driver,
            image: "service:latest".to_owned(),
            host: "127.0.0.1".to_owned(),
            port: 8080,
            database: None,
            username: None,
            password: None,
            bucket: None,
            access_key: None,
            secret_key: None,
            api_key: None,
            region: None,
            scheme: None,
            domain: None,
            domains: None,
            resolved_domain: None,
            routes: None,
            container_port: None,
            smtp_port: None,
            volumes: None,
            env: None,
            command: None,
            depends_on: None,
            seed_file: None,
            hook: Vec::new(),
            health_path: None,
            health_statuses: None,
            restart: None,
            localhost_tls: false,
            octane: false,
            octane_workers: None,
            octane_max_requests: None,
            php_extensions: None,
            trust_container_ca: false,
            env_mapping: None,
            javascript: None,
            container_name: None,
            resolved_container_name: Some("api".to_owned()),
            runtime: Default::default(),
        }
    }

    #[test]
    fn body_health_is_ok_returns_true_for_non_gotenberg_services() {
        assert!(body_health_is_ok(&make_service(Driver::Redis), "unrelated"));
    }

    #[test]
    fn body_health_is_ok_validates_gotenberg_ready_payload() {
        let body = r#"{"status":"UP"}"#;
        assert!(body_health_is_ok(&make_service(Driver::Gotenberg), body));
    }

    #[test]
    fn body_health_is_ok_rejects_invalid_gotenberg_payload() {
        let body = r#"{"status":"starting"}"#;
        assert!(!body_health_is_ok(&make_service(Driver::Gotenberg), body));
    }
}
