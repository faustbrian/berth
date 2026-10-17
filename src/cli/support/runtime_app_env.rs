//! Helpers for app runtime environment composition.

use std::collections::HashMap;

use crate::{config, env};

/// Returns the app runtime env merged with project dependency injection.
///
/// # Errors
///
/// Returns an error when the managed `APP_KEY` cannot be read or created.
pub(crate) fn runtime_app_env(
    config: &config::Config,
    project_dependency_env: &HashMap<String, String>,
) -> anyhow::Result<HashMap<String, String>> {
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
}
