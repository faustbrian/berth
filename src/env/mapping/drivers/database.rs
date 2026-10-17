//! env mapping drivers database module.
//!
//! Contains env mapping drivers database logic used by Berth command workflows.

use std::collections::HashMap;

use crate::config::ServiceConfig;

pub(super) fn apply_database_map(map: &mut HashMap<String, String>, service: &ServiceConfig) {
    map.insert(
        "DB_CONNECTION".to_owned(),
        service.laravel_connection().unwrap_or("mysql").to_owned(),
    );
    map.insert("DB_HOST".to_owned(), service.host.clone());
    map.insert("DB_PORT".to_owned(), service.port.to_string());
    map.insert(
        "DB_DATABASE".to_owned(),
        service.database.clone().unwrap_or_else(|| "app".to_owned()),
    );
    map.insert(
        "DB_USERNAME".to_owned(),
        service
            .username
            .clone()
            .unwrap_or_else(|| "root".to_owned()),
    );
    map.insert(
        "DB_PASSWORD".to_owned(),
        service.password.clone().unwrap_or_default(),
    );
}
