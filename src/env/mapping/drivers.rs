//! env mapping drivers module.
//!
//! Contains env mapping drivers logic used by Berth command workflows.

use std::collections::HashMap;

use crate::config::{Driver, ServiceConfig};

mod cache;
mod database;
mod object_store;
mod search;

pub(super) fn base_map_for_driver(service: &ServiceConfig) -> HashMap<String, String> {
    let mut map = HashMap::new();

    match service.driver {
        Driver::Mongodb | Driver::Postgres | Driver::Mysql | Driver::Sqlserver => {
            database::apply_database_map(&mut map, service)
        }
        Driver::Redis | Driver::Valkey | Driver::Dragonfly | Driver::Memcached => {
            cache::apply_cache_map(&mut map, service)
        }
        Driver::Minio | Driver::Garage | Driver::Rustfs | Driver::Localstack => {
            object_store::apply_object_store_map(&mut map, service)
        }
        Driver::Opensearch => search::apply_opensearch_map(&mut map, service),
        Driver::Elasticsearch => search::apply_elasticsearch_map(&mut map, service),
        Driver::Meilisearch => search::apply_meilisearch_map(&mut map, service),
        Driver::Typesense => search::apply_typesense_map(&mut map, service),
        Driver::Frankenphp
        | Driver::Reverb
        | Driver::Horizon
        | Driver::Scheduler
        | Driver::Dusk
        | Driver::Gotenberg
        | Driver::Mailhog
        | Driver::Rabbitmq
        | Driver::Soketi => {}
    }

    map
}

#[cfg(test)]
mod tests {
    use crate::config::{Driver, Kind, ServiceConfig};

    use super::base_map_for_driver;

    fn build_service(
        name: &str,
        kind: Kind,
        driver: Driver,
        host: &str,
        port: u16,
        scheme: Option<&str>,
    ) -> ServiceConfig {
        ServiceConfig {
            name: name.to_owned(),
            kind,
            driver,
            image: "service:latest".to_owned(),
            host: host.to_owned(),
            port,
            database: Some("app".to_owned()),
            username: Some("admin".to_owned()),
            password: Some("secret".to_owned()),
            bucket: Some("files".to_owned()),
            access_key: Some("access".to_owned()),
            secret_key: Some("secret".to_owned()),
            api_key: Some("api".to_owned()),
            region: Some("us-east-1".to_owned()),
            scheme: scheme.map(ToOwned::to_owned),
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
    fn base_map_for_driver_generates_database_vars_for_sql_drivers() {
        let service = build_service("db", Kind::Database, Driver::Mysql, "127.0.0.1", 3306, None);
        let values = base_map_for_driver(&service);

        assert_eq!(values.get("DB_CONNECTION"), Some(&"mysql".to_owned()));
        assert_eq!(values.get("DB_HOST"), Some(&"127.0.0.1".to_owned()));
        assert_eq!(values.get("DB_PORT"), Some(&"3306".to_owned()));
    }

    #[test]
    fn base_map_for_driver_generates_cache_vars_for_memcached() {
        let service = build_service(
            "cache",
            Kind::Cache,
            Driver::Memcached,
            "127.0.0.1",
            11211,
            None,
        );
        let values = base_map_for_driver(&service);

        assert_eq!(values.get("CACHE_STORE"), Some(&"memcached".to_owned()));
        assert_eq!(values.get("MEMCACHED_HOST"), Some(&"127.0.0.1".to_owned()));
    }

    #[test]
    fn base_map_for_driver_generates_object_store_vars() {
        let service = build_service(
            "files",
            Kind::ObjectStore,
            Driver::Minio,
            "10.0.0.5",
            9000,
            None,
        );
        let values = base_map_for_driver(&service);

        assert_eq!(values.get("AWS_ACCESS_KEY_ID"), Some(&"access".to_owned()));
        assert_eq!(values.get("AWS_BUCKET"), Some(&"files".to_owned()));
        assert_eq!(
            values.get("AWS_USE_PATH_STYLE_ENDPOINT"),
            Some(&"true".to_owned())
        );
    }

    #[test]
    fn base_map_for_driver_generates_search_vars() {
        let opensearch = build_service(
            "search",
            Kind::Search,
            Driver::Opensearch,
            "10.0.0.7",
            9200,
            Some("http"),
        );
        let opensearch_values = base_map_for_driver(&opensearch);
        assert_eq!(
            opensearch_values.get("SCOUT_DRIVER"),
            Some(&"opensearch".to_owned())
        );
        assert_eq!(
            opensearch_values.get("OPENSEARCH_HOST"),
            Some(&"http://10.0.0.7:9200".to_owned())
        );

        let elasticsearch = build_service(
            "search",
            Kind::Search,
            Driver::Elasticsearch,
            "10.0.0.9",
            9200,
            Some("http"),
        );
        let elasticsearch_values = base_map_for_driver(&elasticsearch);
        assert_eq!(
            elasticsearch_values.get("SCOUT_DRIVER"),
            Some(&"elasticsearch".to_owned())
        );
        assert_eq!(
            elasticsearch_values.get("ELASTICSEARCH_HOST"),
            Some(&"http://10.0.0.9:9200".to_owned())
        );

        let service = build_service(
            "search",
            Kind::Search,
            Driver::Typesense,
            "10.0.0.8",
            8108,
            Some("http"),
        );
        let typesense = base_map_for_driver(&service);
        assert_eq!(typesense.get("SCOUT_DRIVER"), Some(&"typesense".to_owned()));
        assert_eq!(
            typesense.get("TYPESENSE_HOST"),
            Some(&"10.0.0.8".to_owned())
        );
        assert_eq!(typesense.get("TYPESENSE_API_KEY"), Some(&"api".to_owned()));
    }

    #[test]
    fn base_map_for_driver_returns_empty_for_no_backend_services() {
        let service = build_service(
            "worker",
            Kind::App,
            Driver::Scheduler,
            "127.0.0.1",
            9000,
            None,
        );
        let values = base_map_for_driver(&service);
        assert!(values.is_empty());
    }
}
