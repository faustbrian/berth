//! cli support resolve up services module.
//!
//! Contains cli support resolve up services logic used by Berth command workflows.

use anyhow::Result;
use std::collections::HashMap;

use crate::config;

use super::select_up_targets;

pub(crate) fn resolve_up_services<'a>(
    config: &'a config::Config,
    service: Option<&'a str>,
    kind: Option<config::Kind>,
    profile: Option<&'a str>,
) -> Result<Vec<&'a config::ServiceConfig>> {
    let selected = select_up_targets(config, service, kind, profile)?;
    let by_name: HashMap<&str, &config::ServiceConfig> = config
        .service
        .iter()
        .map(|service| (service.name.as_str(), service))
        .collect();
    let selected_names: Vec<String> = selected.iter().map(|svc| svc.name.clone()).collect();
    let ordered_names = crate::dependency_order::order_dependency_names(
        &selected_names,
        |current| {
            let Some(service) = by_name.get(current).copied() else {
                anyhow::bail!("service '{current}' is missing");
            };
            Ok(service.depends_on.clone().unwrap_or_default())
        },
        |current| format!("circular dependency detected at service '{current}'"),
    )?;

    ordered_names
        .iter()
        .map(|name| {
            by_name
                .get(name.as_str())
                .copied()
                .ok_or_else(|| anyhow::anyhow!("service '{name}' is missing"))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::resolve_up_services;
    use crate::config;

    fn service(
        name: &str,
        kind: config::Kind,
        driver: config::Driver,
        depends_on: Option<Vec<&str>>,
    ) -> config::ServiceConfig {
        config::ServiceConfig {
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
            container_port: None,
            smtp_port: None,
            volumes: None,
            env: None,
            command: None,
            depends_on: depends_on.map(|items| items.into_iter().map(str::to_owned).collect()),
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

    fn config() -> config::Config {
        config::Config {
            schema_version: 1,
            project_type: config::ProjectType::Project,
            container_prefix: None,
            domain_strategy: None,
            service: vec![
                service("db", config::Kind::Database, config::Driver::Postgres, None),
                service("app", config::Kind::App, config::Driver::Frankenphp, None),
                service(
                    "worker",
                    config::Kind::App,
                    config::Driver::Horizon,
                    Some(vec!["db"]),
                ),
            ],
            swarm: Vec::new(),
        }
    }

    #[test]
    fn resolve_up_services_orders_dependencies_before_dependents() {
        let cfg = config();
        let resolved = resolve_up_services(&cfg, None, None, None).expect("resolve services");
        let names: Vec<String> = resolved.into_iter().map(|svc| svc.name.clone()).collect();
        assert_eq!(names, vec!["db", "app", "worker"]);
    }

    #[test]
    fn resolve_up_services_respects_kind_and_service_filters() {
        let cfg = config();

        let all_apps = resolve_up_services(&cfg, None, Some(config::Kind::App), None)
            .expect("resolve app services");
        let names: Vec<String> = all_apps.into_iter().map(|svc| svc.name.clone()).collect();
        assert_eq!(names, vec!["app", "db", "worker"]);

        let selected = resolve_up_services(&cfg, Some("app"), Some(config::Kind::App), None)
            .expect("resolve selected app");
        assert_eq!(selected[0].name, "app");

        let all_db = resolve_up_services(&cfg, None, Some(config::Kind::Database), None)
            .expect("resolve db services");
        assert_eq!(all_db[0].name, "db");
    }
}
