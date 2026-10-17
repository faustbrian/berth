//! cli support matches filter module.
//!
//! Contains cli support matches filter logic used by Berth command workflows.

use crate::config;

pub(crate) fn matches_filter(
    svc: &config::ServiceConfig,
    kind: Option<config::Kind>,
    driver: Option<config::Driver>,
) -> bool {
    let kind_ok = kind.is_none_or(|k| k == svc.kind);
    let driver_ok = driver.is_none_or(|d| d == svc.driver);
    kind_ok && driver_ok
}

#[cfg(test)]
mod tests {
    use crate::config::{Driver, Kind, ServiceConfig};

    fn app_service(name: &str, kind: Kind, driver: Driver) -> ServiceConfig {
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
    fn matches_filter_matches_when_no_filter_is_set() {
        let svc = app_service("app", Kind::App, Driver::Frankenphp);
        assert!(super::matches_filter(&svc, None, None));
    }

    #[test]
    fn matches_filter_matches_exact_driver_and_kind() {
        let svc = app_service("db", Kind::Database, Driver::Mysql);
        assert!(super::matches_filter(
            &svc,
            Some(Kind::Database),
            Some(Driver::Mysql)
        ));
        assert!(!super::matches_filter(
            &svc,
            Some(Kind::App),
            Some(Driver::Mysql)
        ));
        assert!(!super::matches_filter(
            &svc,
            Some(Kind::Database),
            Some(Driver::Postgres)
        ));
    }
}
