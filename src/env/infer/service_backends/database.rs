//! Database backend env inference.

use std::collections::HashMap;

use crate::config::ServiceConfig;

use super::super::{insert_if_absent, runtime_host_for_app, runtime_port_for_app};

/// Applies inferred Laravel DB variables for SQL services.
///
/// Uses conservative defaults (`mysql`, `app`, `root`) when service config omits
/// optional fields.
pub(super) fn apply(vars: &mut HashMap<String, String>, service: &ServiceConfig) {
    insert_if_absent(
        vars,
        "DB_CONNECTION",
        service.laravel_connection().unwrap_or("mysql").to_owned(),
    );
    insert_if_absent(vars, "DB_HOST", runtime_host_for_app(service));
    insert_if_absent(vars, "DB_PORT", runtime_port_for_app(service).to_string());
    insert_if_absent(
        vars,
        "DB_DATABASE",
        service.database.clone().unwrap_or_else(|| "app".to_owned()),
    );
    insert_if_absent(
        vars,
        "DB_USERNAME",
        service
            .username
            .clone()
            .unwrap_or_else(|| "root".to_owned()),
    );
    insert_if_absent(
        vars,
        "DB_PASSWORD",
        service.password.clone().unwrap_or_default(),
    );
}
