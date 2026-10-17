//! Cache backend env inference (Redis/Valkey).

use std::collections::HashMap;

use crate::config::{Driver, ServiceConfig};

use super::super::{insert_if_absent, runtime_host_for_app, runtime_port_for_app};

/// Applies inferred cache/session/queue variables.
///
/// Populates both default Redis keys and `REDIS_CACHE_*` aliases so Laravel
/// cache and general Redis usage resolve to the same backend by default.
pub(super) fn apply(vars: &mut HashMap<String, String>, service: &ServiceConfig) {
    if service.driver == Driver::Memcached {
        insert_if_absent(vars, "CACHE_STORE", "memcached".to_owned());
        insert_if_absent(vars, "MEMCACHED_HOST", runtime_host_for_app(service));
        insert_if_absent(
            vars,
            "MEMCACHED_PORT",
            runtime_port_for_app(service).to_string(),
        );
        return;
    }

    insert_if_absent(vars, "CACHE_STORE", "redis".to_owned());
    insert_if_absent(vars, "QUEUE_CONNECTION", "redis".to_owned());
    insert_if_absent(vars, "SESSION_DRIVER", "redis".to_owned());
    insert_if_absent(vars, "SESSION_CONNECTION", "default".to_owned());
    insert_if_absent(vars, "REDIS_CLIENT", "phpredis".to_owned());
    let redis_host = runtime_host_for_app(service);
    let redis_port = runtime_port_for_app(service).to_string();
    let redis_username = service.username.clone().unwrap_or_default();
    let redis_password = service.password.clone().unwrap_or_default();

    insert_if_absent(vars, "REDIS_HOST", redis_host.clone());
    insert_if_absent(vars, "REDIS_PORT", redis_port.clone());
    insert_if_absent(vars, "REDIS_USERNAME", redis_username.clone());
    insert_if_absent(vars, "REDIS_PASSWORD", redis_password.clone());
    insert_if_absent(vars, "REDIS_CACHE_HOST", redis_host);
    insert_if_absent(vars, "REDIS_CACHE_PORT", redis_port);
    insert_if_absent(vars, "REDIS_CACHE_USERNAME", redis_username);
    insert_if_absent(vars, "REDIS_CACHE_PASSWORD", redis_password);
}
