//! Test runtime startup helpers for artisan flows.

use anyhow::Result;

use crate::{cli, config, docker};

pub(super) fn resolve_testing_startup_services<'a>(
    config: &'a config::Config,
    selected_service: Option<&'a str>,
) -> Result<Vec<&'a config::ServiceConfig>> {
    let Some(selected_service) = selected_service else {
        return cli::support::resolve_up_services(config, None, None, None);
    };

    let mut selected_names = config
        .service
        .iter()
        .filter(|service| service.kind != config::Kind::App)
        .map(|service| service.name.clone())
        .collect::<Vec<_>>();
    selected_names.push(selected_service.to_owned());

    let by_name = config
        .service
        .iter()
        .map(|service| (service.name.as_str(), service))
        .collect::<std::collections::HashMap<_, _>>();
    let ordered_names = crate::dependency_order::order_dependency_names(
        &selected_names,
        |current| {
            let Some(service) = by_name.get(current).copied() else {
                anyhow::bail!("service '{current}' is missing");
            };
            Ok(service.depends_on.clone().unwrap_or_default())
        },
        |path| format!("circular service dependency: {path}"),
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

pub(super) fn run_testing_startup_services<F>(
    startup_services: &[&config::ServiceConfig],
    start_context: &cli::support::ServiceStartContext<'_>,
    reset_service_runtime: F,
) -> Result<()>
where
    F: Fn(&config::ServiceConfig) -> Result<()>,
{
    for svc in startup_services {
        let wait_healthy = svc.kind != config::Kind::App;
        reset_service_runtime(svc)?;
        cli::support::start_service(
            svc,
            start_context,
            false,
            docker::PullPolicy::Missing,
            wait_healthy,
            None,
            true,
        )?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::resolve_testing_startup_services;
    use crate::config::{Config, Driver, Kind, ProjectType, ServiceConfig};

    #[test]
    fn resolve_testing_startup_services_keeps_infra_and_selected_app_only() {
        let config = Config {
            schema_version: 1,
            project_type: ProjectType::Project,
            container_prefix: None,
            domain_strategy: None,
            service: vec![
                service("db", Kind::Database, Driver::Postgres, None),
                service("cache", Kind::Cache, Driver::Valkey, None),
                service("app", Kind::App, Driver::Frankenphp, Some(vec!["db"])),
                service(
                    "profile-app",
                    Kind::App,
                    Driver::Frankenphp,
                    Some(vec!["db"]),
                ),
            ],
            swarm: Vec::new(),
        };

        let scoped =
            resolve_testing_startup_services(&config, Some("app")).expect("resolve services");
        let names: Vec<&str> = scoped.iter().map(|service| service.name.as_str()).collect();

        assert_eq!(names, vec!["db", "cache", "app"]);
    }

    fn service(
        name: &str,
        kind: Kind,
        driver: Driver,
        depends_on: Option<Vec<&str>>,
    ) -> ServiceConfig {
        ServiceConfig {
            name: name.to_owned(),
            kind,
            driver,
            image: "test-image".to_owned(),
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
            runtime: Default::default(),
        }
    }
}
