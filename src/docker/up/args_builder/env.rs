//! docker up args builder env module.
//!
//! Contains docker up args builder env logic used by Berth command workflows.

use crate::config::{Driver, ServiceConfig};

mod common;
mod object_store;
mod search;
mod sql;
use common::{append_custom_env, append_volumes};

/// Appends run options to the caller-provided command or collection.
pub(super) fn append_run_options(
    args: &mut Vec<String>,
    service: &ServiceConfig,
    container_name: &str,
) {
    append_volumes(args, service, container_name);
    append_custom_env(args, service);
    append_driver_env(args, service);
}

/// Appends driver env to the caller-provided command or collection.
fn append_driver_env(args: &mut Vec<String>, service: &ServiceConfig) {
    match service.driver {
        Driver::Mongodb | Driver::Postgres | Driver::Mysql | Driver::Sqlserver => {
            sql::append(args, service)
        }
        Driver::Opensearch | Driver::Elasticsearch | Driver::Meilisearch | Driver::Typesense => {
            search::append(args, service)
        }
        Driver::Minio | Driver::Garage | Driver::Rustfs | Driver::Localstack => {
            object_store::append(args, service)
        }
        Driver::Rabbitmq => append_rabbitmq(args, service),
        Driver::Redis
        | Driver::Valkey
        | Driver::Dragonfly
        | Driver::Memcached
        | Driver::Frankenphp
        | Driver::Reverb
        | Driver::Horizon
        | Driver::Scheduler
        | Driver::Dusk
        | Driver::Gotenberg
        | Driver::Mailhog
        | Driver::Soketi => {}
    }
}

fn append_rabbitmq(args: &mut Vec<String>, service: &ServiceConfig) {
    args.push("-e".to_owned());
    args.push(format!(
        "RABBITMQ_DEFAULT_USER={}",
        service.username.as_deref().unwrap_or("guest")
    ));
    args.push("-e".to_owned());
    args.push(format!(
        "RABBITMQ_DEFAULT_PASS={}",
        service.password.as_deref().unwrap_or("guest")
    ));
}

#[cfg(test)]
mod tests {
    use super::append_run_options;
    use crate::config::{Driver, Kind, ServiceConfig};

    fn service() -> ServiceConfig {
        ServiceConfig {
            name: "db".to_owned(),
            kind: Kind::Database,
            driver: Driver::Mysql,
            image: "mysql:8.1".to_owned(),
            host: "127.0.0.1".to_owned(),
            port: 3306,
            database: Some("laravel".to_owned()),
            username: Some("laravel".to_owned()),
            password: Some("laravel".to_owned()),
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
            container_name: Some("acme-db".to_owned()),
            resolved_container_name: Some("acme-db".to_owned()),
            runtime: Default::default(),
        }
    }

    #[test]
    fn adds_default_named_data_volume_for_stateful_services() {
        let mut args = Vec::new();
        append_run_options(&mut args, &service(), "acme-db");
        let rendered = args.join(" ");

        assert!(rendered.contains("-v acme-db-data:/var/lib/mysql"));
    }

    #[test]
    fn keeps_explicit_volumes_without_default_data_volume() {
        let mut svc = service();
        svc.volumes = Some(vec!["./data:/var/lib/mysql".to_owned()]);
        let mut args = Vec::new();
        append_run_options(&mut args, &svc, "acme-db");
        let rendered = args.join(" ");

        assert!(rendered.contains("-v ./data:/var/lib/mysql"));
        assert!(!rendered.contains("acme-db-data:/var/lib/mysql"));
    }
}
