//! Docker ops JSON output helpers.

use anyhow::Result;
use serde::Serialize;

use crate::cli::handlers::serialize;
use crate::config;

pub(super) fn print_pretty_json<T: Serialize>(value: &T) -> Result<()> {
    serialize::print_json_pretty(value)
}

pub(super) fn collect_service_json(
    services: Vec<&config::ServiceConfig>,
    map_service: impl Fn(&config::ServiceConfig) -> Result<Vec<serde_json::Value>>,
) -> Result<Vec<serde_json::Value>> {
    let mut items = Vec::new();
    for service in services {
        items.extend(map_service(service)?);
    }
    Ok(items)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::config::{Kind, ServiceConfig};

    use super::{collect_service_json, print_pretty_json};

    #[test]
    fn collect_service_json_collects_mapped_values() -> anyhow::Result<()> {
        let service = ServiceConfig {
            name: "web".to_owned(),
            kind: Kind::App,
            driver: crate::config::Driver::Frankenphp,
            image: "dunglas/frankenphp:php8.5".to_owned(),
            host: "127.0.0.1".to_owned(),
            port: 80,
            database: None,
            username: None,
            password: None,
            bucket: None,
            access_key: None,
            secret_key: None,
            api_key: None,
            region: None,
            scheme: None,
            domain: Some("app.localhost".to_owned()),
            domains: None,
            resolved_domain: None,
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
            resolved_container_name: None,
            runtime: Default::default(),
        };

        let values = collect_service_json(vec![&service], |svc| {
            Ok(vec![json!({
                "name": svc.name,
                "image": svc.image,
            })])
        })?;
        assert_eq!(
            values,
            vec![json!({"name":"web","image":"dunglas/frankenphp:php8.5"})]
        );
        Ok(())
    }

    #[test]
    fn collect_service_json_propagates_mapping_failure() {
        let service = ServiceConfig {
            name: "web".to_owned(),
            kind: Kind::App,
            driver: crate::config::Driver::Frankenphp,
            image: "dunglas/frankenphp:php8.5".to_owned(),
            host: "127.0.0.1".to_owned(),
            port: 80,
            database: None,
            username: None,
            password: None,
            bucket: None,
            access_key: None,
            secret_key: None,
            api_key: None,
            region: None,
            scheme: None,
            domain: Some("app.localhost".to_owned()),
            domains: None,
            resolved_domain: None,
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
            resolved_container_name: None,
            runtime: Default::default(),
        };

        let error =
            collect_service_json(vec![&service], |_| Err(anyhow::anyhow!("mapping failed")))
                .expect_err("mapping failure should bubble up");
        assert_eq!(error.to_string(), "mapping failed");
    }

    #[test]
    fn print_pretty_json_preserves_shape() -> anyhow::Result<()> {
        let value = json!({"status":"ok"});
        print_pretty_json(&value)?;
        Ok(())
    }
}
