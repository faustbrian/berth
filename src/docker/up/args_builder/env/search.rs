//! docker up args builder env search module.
//!
//! Contains docker up args builder env search logic used by Berth command workflows.

use crate::config::{Driver, ServiceConfig};

/// Appends append to the caller-provided command or collection.
pub(super) fn append(args: &mut Vec<String>, service: &ServiceConfig) {
    match service.driver {
        Driver::Meilisearch => {
            if let Some(api_key) = &service.api_key {
                args.push("-e".to_owned());
                args.push(format!("MEILI_MASTER_KEY={api_key}"));
            }
        }
        Driver::Typesense => {
            if let Some(api_key) = &service.api_key {
                args.push("-e".to_owned());
                args.push(format!("TYPESENSE_API_KEY={api_key}"));
            }
        }
        _ => {}
    }
}
