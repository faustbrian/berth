//! config services module.
//!
//! Contains config services logic used by Berth command workflows.

use super::Config;

mod lookup;

pub(crate) use lookup::{find_service, resolve_app_service, resolve_service};

pub(crate) fn available_service_names(config: &Config) -> Vec<&str> {
    config.service.iter().map(|svc| svc.name.as_str()).collect()
}
