//! config service methods ports module.
//!
//! Contains config service methods ports logic used by Berth command workflows.

use super::{Driver, ServiceConfig};

impl ServiceConfig {
    /// Returns the default internal service port for the driver.
    #[must_use]
    pub fn default_port(&self) -> u16 {
        let driver_port = match self.driver {
            Driver::Mongodb => 27017,
            Driver::Memcached => 11211,
            Driver::Postgres => 5432,
            Driver::Mysql => 3306,
            Driver::Sqlserver => 1433,
            Driver::Redis | Driver::Valkey | Driver::Dragonfly => 6379,
            Driver::Garage => 3900,
            Driver::Rustfs => 9000,
            Driver::Localstack => 4566,
            Driver::Opensearch | Driver::Elasticsearch => 9200,
            Driver::Meilisearch => 7700,
            Driver::Typesense => 8108,
            Driver::Frankenphp => 80,
            Driver::Reverb => 8080,
            Driver::Horizon => 8000,
            Driver::Scheduler => 8001,
            Driver::Dusk => 4444,
            Driver::Gotenberg => 3000,
            Driver::Mailhog => 8025,
            Driver::Rabbitmq => 5672,
            Driver::Soketi => 6001,
        };
        self.container_port.unwrap_or(driver_port)
    }

    /// Returns true when this is a SQL database service.
    #[must_use]
    pub const fn is_database(&self) -> bool {
        matches!(
            self.driver,
            Driver::Postgres | Driver::Mysql | Driver::Sqlserver
        )
    }

    /// Returns true when this service supports dump/restore.
    #[must_use]
    pub const fn supports_sql_dump(&self) -> bool {
        matches!(self.driver, Driver::Postgres | Driver::Mysql)
    }

    /// Laravel DB_CONNECTION-like driver name.
    #[must_use]
    pub const fn laravel_connection(&self) -> Option<&str> {
        match self.driver {
            Driver::Mongodb => Some("mongodb"),
            Driver::Postgres => Some("pgsql"),
            Driver::Mysql => Some("mysql"),
            Driver::Sqlserver => Some("sqlsrv"),
            Driver::Redis | Driver::Valkey | Driver::Dragonfly => Some("redis"),
            Driver::Garage
            | Driver::Memcached
            | Driver::Rustfs
            | Driver::Localstack
            | Driver::Opensearch
            | Driver::Elasticsearch
            | Driver::Meilisearch
            | Driver::Typesense
            | Driver::Frankenphp
            | Driver::Reverb
            | Driver::Horizon
            | Driver::Scheduler
            | Driver::Dusk
            | Driver::Gotenberg
            | Driver::Mailhog
            | Driver::Rabbitmq
            | Driver::Soketi => None,
        }
    }

    /// Returns internal container port.
    #[must_use]
    pub fn resolved_container_port(&self) -> u16 {
        self.default_port()
    }
}
