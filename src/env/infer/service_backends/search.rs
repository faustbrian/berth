//! Search backend env inference.

use std::collections::HashMap;

use crate::config::{Driver, ServiceConfig};

use super::super::{
    insert_if_absent, runtime_host_for_app, runtime_port_for_app, service_endpoint,
};

/// Applies inferred Scout/search variables based on selected search driver.
pub(super) fn apply(vars: &mut HashMap<String, String>, service: &ServiceConfig) {
    match service.driver {
        Driver::Opensearch => apply_opensearch(vars, service),
        Driver::Elasticsearch => apply_elasticsearch(vars, service),
        Driver::Meilisearch => apply_meilisearch(vars, service),
        Driver::Typesense => apply_typesense(vars, service),
        _ => {}
    }
}

fn apply_opensearch(vars: &mut HashMap<String, String>, service: &ServiceConfig) {
    insert_if_absent(vars, "SCOUT_DRIVER", "opensearch".to_owned());
    insert_if_absent(vars, "OPENSEARCH_HOST", service_endpoint(service));
}

fn apply_elasticsearch(vars: &mut HashMap<String, String>, service: &ServiceConfig) {
    insert_if_absent(vars, "SCOUT_DRIVER", "elasticsearch".to_owned());
    insert_if_absent(vars, "ELASTICSEARCH_HOST", service_endpoint(service));
}

/// Applies Meilisearch-specific env keys.
fn apply_meilisearch(vars: &mut HashMap<String, String>, service: &ServiceConfig) {
    insert_if_absent(vars, "SCOUT_DRIVER", "meilisearch".to_owned());
    insert_if_absent(vars, "MEILISEARCH_HOST", service_endpoint(service));
    insert_if_absent(
        vars,
        "MEILISEARCH_KEY",
        service.api_key.clone().unwrap_or_default(),
    );
}

/// Applies Typesense-specific env keys.
fn apply_typesense(vars: &mut HashMap<String, String>, service: &ServiceConfig) {
    insert_if_absent(vars, "SCOUT_DRIVER", "typesense".to_owned());
    insert_if_absent(vars, "TYPESENSE_HOST", runtime_host_for_app(service));
    insert_if_absent(
        vars,
        "TYPESENSE_PORT",
        runtime_port_for_app(service).to_string(),
    );
    insert_if_absent(vars, "TYPESENSE_PROTOCOL", service.scheme().to_owned());
    insert_if_absent(
        vars,
        "TYPESENSE_API_KEY",
        service.api_key.clone().unwrap_or_default(),
    );
}
