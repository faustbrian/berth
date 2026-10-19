//! Helpers for app runtime environment composition.

use std::collections::HashMap;

use crate::{config, env};

/// Returns the app runtime env merged with project dependency injection.
///
/// # Errors
///
/// Returns an error when a swarm injects a var one of the project's
/// `routes` sets, or when the managed `APP_KEY` cannot be read or created.
pub(crate) fn runtime_app_env(
    config: &config::Config,
    project_dependency_env: &HashMap<String, String>,
) -> anyhow::Result<HashMap<String, String>> {
    // Injected values replace inferred ones, so a route's URL would be lost.
    // The injecting workspace is resolved only here, not at config load.
    for service in &config.service {
        for (label, var) in service.routes.iter().flatten() {
            if project_dependency_env.contains_key(var) {
                anyhow::bail!(
                    "service '{}' route '{label}' sets {var}, which the swarm workspace also \
                     injects into this project; remove one of them",
                    service.name
                );
            }
        }
    }
    let mut inferred_env = env::inferred_app_env(config);
    env::merge_with_protected_https_app_urls(&mut inferred_env, project_dependency_env);
    env::apply_managed_runtime_env(config, &mut inferred_env)?;
    Ok(inferred_env)
}

#[cfg(test)]
mod tests {
    use super::runtime_app_env;
    use crate::config;
    use crate::config::{Driver, Kind, ServiceConfig};
    use std::collections::HashMap;

    fn app_service() -> ServiceConfig {
        ServiceConfig {
            name: "app".to_owned(),
            kind: Kind::App,
            driver: Driver::Frankenphp,
            image: "dunglas/frankenphp:php8.5".to_owned(),
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
            domain: Some("app.berth".to_owned()),
            domains: None,
            resolved_domain: None,
            routes: None,
            container_port: Some(80),
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
            resolved_container_name: Some("app-service".to_owned()),
            runtime: Default::default(),
        }
    }

    #[test]
    fn runtime_app_env_keeps_https_values_from_inferred_urls() {
        let mut config = config::Config {
            schema_version: 1,
            project_type: config::ProjectType::Project,
            container_prefix: None,
            domain_strategy: None,
            service: vec![app_service()],
            swarm: Vec::new(),
        };
        config.service[0].localhost_tls = true;

        let injected = HashMap::from([
            ("APP_URL".to_owned(), "http://example.dev".to_owned()),
            ("ASSET_URL".to_owned(), "http://example.dev".to_owned()),
            ("CUSTOM_KEY".to_owned(), "value".to_owned()),
        ]);

        let env = runtime_app_env(&config, &injected).expect("env");

        assert_eq!(
            env.get("APP_URL"),
            Some(&"https://localhost:8080".to_owned())
        );
        assert_eq!(
            env.get("ASSET_URL"),
            Some(&"https://localhost:8080".to_owned())
        );
        assert_eq!(env.get("CUSTOM_KEY"), Some(&"value".to_owned()));
    }

    #[test]
    fn an_injected_var_that_a_route_sets_fails_instead_of_replacing_its_url() {
        let mut app = app_service();
        app.routes = Some(std::collections::BTreeMap::from([(
            "ee".to_owned(),
            "APP_EE_URL".to_owned(),
        )]));
        let config = config::Config {
            schema_version: 1,
            project_type: config::ProjectType::Project,
            container_prefix: None,
            domain_strategy: None,
            service: vec![app],
            swarm: Vec::new(),
        };
        let injected = HashMap::from([("APP_EE_URL".to_owned(), "http://api:80".to_owned())]);

        let error = runtime_app_env(&config, &injected).expect_err("route var injected");
        let message = format!("{error:#}");
        assert!(
            message.contains("APP_EE_URL") && message.contains("route 'ee'"),
            "{message}"
        );
        assert!(
            runtime_app_env(
                &config,
                &HashMap::from([("OTHER".to_owned(), "x".to_owned())])
            )
            .is_ok()
        );
    }
}
