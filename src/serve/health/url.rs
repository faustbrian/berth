//! Health endpoint URL construction for serve targets.

use anyhow::Result;

use crate::config::ServiceConfig;

/// Builds the full health URL from target public URL and health path settings.
pub(super) fn health_url_for_target(
    target: &ServiceConfig,
    health_path: Option<&str>,
) -> Result<String> {
    let base = super::super::public_url(target)?;
    let configured_path = health_path
        .or(target.health_path.as_deref())
        .unwrap_or("/up");
    let path = if configured_path.starts_with('/') {
        configured_path.to_owned()
    } else {
        format!("/{configured_path}")
    };

    Ok(format!("{}{}", base.trim_end_matches('/'), path))
}

#[cfg(test)]
mod tests {
    use super::health_url_for_target;
    use crate::config::{Driver, Kind, ServiceConfig};

    fn make_target() -> ServiceConfig {
        ServiceConfig {
            name: "api".to_owned(),
            kind: Kind::App,
            driver: Driver::Frankenphp,
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
            scheme: Some("https".to_owned()),
            domain: Some("app.berth".to_owned()),
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
            container_name: Some("api".to_owned()),
            resolved_container_name: Some("api".to_owned()),
        }
    }

    #[test]
    fn health_url_for_target_normalizes_path_without_leading_slash() {
        let target = make_target();
        let url = health_url_for_target(&target, Some("ready")).expect("url");
        assert_eq!(url, "https://app.berth/ready");
    }

    #[test]
    fn health_url_for_target_uses_target_health_path_when_not_overridden() {
        let mut target = make_target();
        target.health_path = Some("ready".to_owned());
        let url = health_url_for_target(&target, None).expect("url");
        assert_eq!(url, "https://app.berth/ready");
    }
}
