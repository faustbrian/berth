//! config api sql client flavor module.
//!
//! Contains config api sql client flavor logic used by Berth command workflows.

use crate::config::{Config, Driver, Kind};

/// Returns preferred SQL client flavor for app-side tooling (`mysql` or `mariadb`).
///
/// The choice is inferred from configured SQL service images:
/// - `mariadb` when all configured MySQL-driver database images are MariaDB images.
/// - `mysql` otherwise (default and mixed environments).
#[must_use]
pub fn preferred_sql_client_flavor(config: &Config) -> &'static str {
    let mut saw_mysql_driver_db = false;
    let mut saw_mariadb_image = false;
    let mut saw_mysql_image = false;

    for service in &config.service {
        if service.kind != Kind::Database || service.driver != Driver::Mysql {
            continue;
        }
        saw_mysql_driver_db = true;
        let image = service.image.trim().to_ascii_lowercase();
        if image.starts_with("mariadb:") || image.starts_with("mariadb/") || image == "mariadb" {
            saw_mariadb_image = true;
            continue;
        }
        if image.starts_with("mysql:") || image.starts_with("mysql/") || image == "mysql" {
            saw_mysql_image = true;
        }
    }

    if saw_mysql_driver_db && saw_mariadb_image && !saw_mysql_image {
        return "mariadb";
    }

    "mysql"
}

#[cfg(test)]
mod tests {
    use super::preferred_sql_client_flavor;
    use crate::config::{Config, Driver, Kind, ServiceConfig};

    fn sql_service(image: &str) -> ServiceConfig {
        ServiceConfig {
            name: "db".to_owned(),
            kind: Kind::Database,
            driver: Driver::Mysql,
            image: image.to_owned(),
            host: "127.0.0.1".to_owned(),
            port: 33060,
            database: Some("app".to_owned()),
            username: Some("root".to_owned()),
            password: Some("secret".to_owned()),
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

    #[test]
    fn defaults_to_mysql_without_sql_services() {
        let config = Config {
            schema_version: 1,
            project_type: crate::config::ProjectType::Project,
            container_prefix: Some("app".to_owned()),
            domain_strategy: None,
            service: vec![],
            swarm: vec![],
        };
        assert_eq!(preferred_sql_client_flavor(&config), "mysql");
    }

    #[test]
    fn picks_mysql_for_mysql_images() {
        let config = Config {
            schema_version: 1,
            project_type: crate::config::ProjectType::Project,
            container_prefix: Some("app".to_owned()),
            domain_strategy: None,
            service: vec![sql_service("mysql:8.1")],
            swarm: vec![],
        };
        assert_eq!(preferred_sql_client_flavor(&config), "mysql");
    }

    #[test]
    fn picks_mariadb_for_mariadb_only_images() {
        let config = Config {
            schema_version: 1,
            project_type: crate::config::ProjectType::Project,
            container_prefix: Some("app".to_owned()),
            domain_strategy: None,
            service: vec![sql_service("mariadb:11")],
            swarm: vec![],
        };
        assert_eq!(preferred_sql_client_flavor(&config), "mariadb");
    }
}
