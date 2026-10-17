//! config validation container names module.
//!
//! Contains config validation container names logic used by Berth command workflows.

use anyhow::Result;
use std::collections::HashSet;

use crate::config::Config;

/// Validates and resolves container names and reports actionable failures.
pub(super) fn validate_and_resolve_container_names(config: &mut Config) -> Result<()> {
    let prefix = config
        .container_prefix
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);

    if config.service.is_empty() {
        return Ok(());
    }

    let mut missing = Vec::new();
    let mut resolved_names = HashSet::new();

    for service in &mut config.service {
        let explicit = service
            .container_name
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned);

        let resolved = if let Some(explicit_name) = explicit {
            explicit_name
        } else if let Some(prefix_value) = &prefix {
            format!("{prefix_value}-{}", service.name)
        } else {
            missing.push(service.name.clone());
            continue;
        };

        if !resolved_names.insert(resolved.clone()) {
            anyhow::bail!("duplicate container name resolved: '{resolved}'");
        }

        service.resolved_container_name = Some(resolved);
    }

    if !missing.is_empty() {
        anyhow::bail!(
            "missing container naming strategy: set `container_prefix` or \
             `container_name` on each service (missing: {})",
            missing.join(", ")
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::validate_and_resolve_container_names;
    use crate::config::{Config, Driver, Kind, ServiceConfig};

    fn app_service(name: &str, host: &str, explicit_container_name: Option<&str>) -> ServiceConfig {
        ServiceConfig {
            name: name.to_owned(),
            kind: Kind::App,
            driver: Driver::Frankenphp,
            image: "dunglas/frankenphp:php8.5".to_owned(),
            host: host.to_owned(),
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
            domain: None,
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
            container_name: explicit_container_name.map(ToOwned::to_owned),
            resolved_container_name: None,
        }
    }

    #[test]
    fn resolve_container_names_requires_prefix_or_explicit_name() {
        let mut config = Config {
            schema_version: 1,
            project_type: crate::config::ProjectType::Project,
            container_prefix: None,
            domain_strategy: None,
            service: vec![app_service("api", "127.0.0.1", None)],
            swarm: Vec::new(),
        };
        let missing = validate_and_resolve_container_names(&mut config);
        assert!(missing.is_err());
    }

    #[test]
    fn resolve_container_names_prefers_explicit_names() {
        let mut config = Config {
            schema_version: 1,
            project_type: crate::config::ProjectType::Project,
            container_prefix: Some("berth".to_owned()),
            domain_strategy: None,
            service: vec![
                app_service("api", "127.0.0.1", Some("custom-api")),
                app_service("web", "127.0.0.1", None),
            ],
            swarm: Vec::new(),
        };

        validate_and_resolve_container_names(&mut config).expect("resolve names");
        assert_eq!(
            config.service[0].resolved_container_name.as_deref(),
            Some("custom-api")
        );
        assert_eq!(
            config.service[1].resolved_container_name.as_deref(),
            Some("berth-web")
        );
    }

    #[test]
    fn resolve_container_names_detects_duplicates() {
        let mut config = Config {
            schema_version: 1,
            project_type: crate::config::ProjectType::Project,
            container_prefix: Some("berth".to_owned()),
            domain_strategy: None,
            service: vec![
                app_service("api", "127.0.0.1", Some("api")),
                app_service("web", "127.0.0.1", Some("api")),
            ],
            swarm: Vec::new(),
        };

        let duplicated = validate_and_resolve_container_names(&mut config);
        assert!(duplicated.is_err());
    }
}
