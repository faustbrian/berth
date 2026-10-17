//! cli support resolve profile targets module.
//!
//! Contains cli support resolve profile targets logic used by Berth command workflows.

use anyhow::Result;

use crate::config;

pub(crate) fn resolve_profile_targets<'a>(
    config: &'a config::Config,
    profile: &str,
) -> Result<Vec<&'a config::ServiceConfig>> {
    let targets = match profile {
        "all" | "full" => config.service.iter().collect(),
        "infra" => config
            .service
            .iter()
            .filter(|svc| svc.kind != config::Kind::App)
            .collect(),
        "data" => config
            .service
            .iter()
            .filter(|svc| {
                matches!(
                    svc.kind,
                    config::Kind::Database
                        | config::Kind::Cache
                        | config::Kind::ObjectStore
                        | config::Kind::Search
                )
            })
            .collect(),
        "app" => config
            .service
            .iter()
            .filter(|svc| svc.kind == config::Kind::App)
            .collect(),
        "web" | "api" => {
            let primary_app = config::resolve_app_service(config, None)?;
            vec![primary_app]
        }
        _ => {
            anyhow::bail!(
                "unknown profile '{profile}'. expected one of: full, all, infra, data, app, web, api"
            )
        }
    };

    Ok(targets)
}

#[cfg(test)]
mod tests {
    use super::resolve_profile_targets;
    use crate::config::{Config, Driver, Kind, ServiceConfig};

    fn service(name: &str, kind: Kind) -> ServiceConfig {
        ServiceConfig {
            name: name.to_owned(),
            kind,
            driver: Driver::Frankenphp,
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
        }
    }

    fn config() -> Config {
        Config {
            schema_version: 1,
            project_type: crate::config::ProjectType::Project,
            container_prefix: None,
            domain_strategy: None,
            service: vec![
                service("app", Kind::App),
                service("db", Kind::Database),
                service("search", Kind::Search),
            ],
            swarm: Vec::new(),
        }
    }

    #[test]
    fn all_and_full_return_every_service() {
        let config = config();
        let all = resolve_profile_targets(&config, "all").expect("all profile");
        let full = resolve_profile_targets(&config, "full").expect("full profile");
        assert_eq!(all.len(), 3);
        assert_eq!(full.len(), 3);
    }

    #[test]
    fn infra_profile_excludes_app_services() {
        let config = config();
        let infra = resolve_profile_targets(&config, "infra").expect("infra profile");
        assert_eq!(infra.len(), 2);
        assert!(infra.iter().all(|svc| svc.kind != Kind::App));
    }

    #[test]
    fn data_profile_includes_data_services() {
        let config = config();
        let data = resolve_profile_targets(&config, "data").expect("data profile");
        assert_eq!(data.len(), 2);
        assert_eq!(data[0].name, "db");
        assert_eq!(data[1].name, "search");
    }

    #[test]
    fn unknown_profile_errors() {
        let config = config();
        assert!(resolve_profile_targets(&config, "invalid").is_err());
    }
}
