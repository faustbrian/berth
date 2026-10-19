//! Shared target selection for up-like service operations.

use anyhow::Result;

use crate::config;

use super::resolve_profile_targets;
use super::selected_services;

pub(crate) fn select_up_targets<'a>(
    config: &'a config::Config,
    service: Option<&'a str>,
    kind: Option<config::Kind>,
    profile: Option<&'a str>,
) -> Result<Vec<&'a config::ServiceConfig>> {
    if let Some(profile_name) = profile {
        return resolve_profile_targets(config, profile_name);
    }

    selected_services(config, service, kind, None)
}

#[cfg(test)]
mod tests {
    use crate::config::{Config, Driver, Kind, ServiceConfig};

    use super::select_up_targets;

    fn service(name: &str, kind: Kind, driver: Driver) -> ServiceConfig {
        ServiceConfig {
            name: name.to_owned(),
            kind,
            driver,
            image: "php".to_owned(),
            host: "127.0.0.1".to_owned(),
            port: 9000,
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
            resolved_container_name: None,
            runtime: Default::default(),
        }
    }

    fn config() -> Config {
        Config {
            schema_version: 1,
            project_type: crate::config::ProjectType::Project,
            container_prefix: None,
            domain_strategy: None,
            service: vec![service("api", Kind::App, Driver::Frankenphp)],
            swarm: Vec::new(),
        }
    }

    #[test]
    fn select_up_targets_uses_profile_targets() {
        let config = config();
        let selected = select_up_targets(&config, None, None, Some("all")).expect("all profile");
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].name, "api");
    }

    #[test]
    fn select_up_targets_returns_selected_services() {
        let config = config();
        let selected = select_up_targets(&config, Some("api"), Some(Kind::App), None)
            .expect("selected app service");
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].name, "api");
    }
}
