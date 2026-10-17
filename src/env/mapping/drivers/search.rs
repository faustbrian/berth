//! env mapping drivers search module.
//!
//! Contains env mapping drivers search logic used by Berth command workflows.

use std::collections::HashMap;

use crate::config::ServiceConfig;

pub(super) fn apply_opensearch_map(map: &mut HashMap<String, String>, service: &ServiceConfig) {
    map.insert("SCOUT_DRIVER".to_owned(), "opensearch".to_owned());
    map.insert(
        "OPENSEARCH_HOST".to_owned(),
        format!("{}://{}:{}", service.scheme(), service.host, service.port),
    );
}

pub(super) fn apply_elasticsearch_map(map: &mut HashMap<String, String>, service: &ServiceConfig) {
    map.insert("SCOUT_DRIVER".to_owned(), "elasticsearch".to_owned());
    map.insert(
        "ELASTICSEARCH_HOST".to_owned(),
        format!("{}://{}:{}", service.scheme(), service.host, service.port),
    );
}

pub(super) fn apply_meilisearch_map(map: &mut HashMap<String, String>, service: &ServiceConfig) {
    map.insert("SCOUT_DRIVER".to_owned(), "meilisearch".to_owned());
    map.insert(
        "MEILISEARCH_HOST".to_owned(),
        format!("{}://{}:{}", service.scheme(), service.host, service.port),
    );
    map.insert(
        "MEILISEARCH_KEY".to_owned(),
        service.api_key.clone().unwrap_or_default(),
    );
}

pub(super) fn apply_typesense_map(map: &mut HashMap<String, String>, service: &ServiceConfig) {
    map.insert("SCOUT_DRIVER".to_owned(), "typesense".to_owned());
    map.insert("TYPESENSE_HOST".to_owned(), service.host.clone());
    map.insert("TYPESENSE_PORT".to_owned(), service.port.to_string());
    map.insert("TYPESENSE_PROTOCOL".to_owned(), service.scheme().to_owned());
    map.insert(
        "TYPESENSE_API_KEY".to_owned(),
        service.api_key.clone().unwrap_or_default(),
    );
}
