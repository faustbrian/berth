//! Env inference dispatcher for backend service drivers.

use std::collections::HashMap;

use crate::config::{Driver, ServiceConfig};
use crate::env::mapping::apply_mapping;

use super::insert_if_absent;
mod cache;
mod database;
mod object_store;
mod search;

/// Applies inferred backend env variables for a single non-app service.
pub(super) fn apply_service_env(vars: &mut HashMap<String, String>, service: &ServiceConfig) {
    let mut service_vars = HashMap::new();
    match service.driver {
        Driver::Mongodb | Driver::Postgres | Driver::Mysql | Driver::Sqlserver => {
            database::apply(&mut service_vars, service)
        }
        Driver::Redis | Driver::Valkey | Driver::Dragonfly | Driver::Memcached => {
            cache::apply(&mut service_vars, service)
        }
        Driver::Garage | Driver::Rustfs | Driver::Localstack => {
            object_store::apply(&mut service_vars, service)
        }
        Driver::Opensearch | Driver::Elasticsearch | Driver::Meilisearch | Driver::Typesense => {
            search::apply(&mut service_vars, service)
        }
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

    apply_mapping(&mut service_vars, service.env_mapping.as_ref());
    for (key, value) in service_vars {
        insert_if_absent(vars, &key, value);
    }
}
