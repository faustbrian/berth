//! Shared service selection/iteration scope for CLI commands.

use anyhow::Result;

use crate::config;

pub(crate) struct ServiceScope<'a> {
    config: &'a config::Config,
    service: Option<&'a str>,
    kind: Option<config::Kind>,
}

impl<'a> ServiceScope<'a> {
    pub(crate) const fn new(
        config: &'a config::Config,
        service: Option<&'a str>,
        kind: Option<config::Kind>,
    ) -> Self {
        Self {
            config,
            service,
            kind,
        }
    }

    pub(crate) fn selected(&self) -> Result<Vec<&'a config::ServiceConfig>> {
        super::selected_services(self.config, self.service, self.kind, None)
    }

    pub(crate) fn for_each<F>(&self, parallel: usize, callback: F) -> Result<()>
    where
        F: Fn(&config::ServiceConfig) -> Result<()> + Send + Sync,
    {
        super::for_each_service(
            self.config,
            self.service,
            self.kind,
            None,
            parallel,
            callback,
        )
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use crate::config::{Config, Driver, Kind, ServiceConfig};

    use super::ServiceScope;

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
            service: vec![
                service("api", Kind::App, Driver::Frankenphp),
                service("db", Kind::Database, Driver::Mysql),
            ],
            swarm: Vec::new(),
        }
    }

    #[test]
    fn selected_returns_matching_services() {
        let config = config();
        let scope = ServiceScope::new(&config, Some("db"), Some(Kind::Database));
        let selected = scope.selected().expect("selected services");
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].name, "db");
    }

    #[test]
    fn for_each_applies_callback() {
        let config = config();
        let scope = ServiceScope::new(&config, None, None);
        let seen = Arc::new(Mutex::new(Vec::new()));
        let clone = Arc::clone(&seen);

        scope
            .for_each(2, |svc| {
                clone.lock().expect("seen lock").push(svc.name.clone());
                Ok(())
            })
            .expect("for each callback");

        let seen = seen.lock().expect("seen lock");
        assert_eq!(seen.len(), 2);
        assert!(seen.contains(&"api".to_string()));
        assert!(seen.contains(&"db".to_string()));
    }
}
