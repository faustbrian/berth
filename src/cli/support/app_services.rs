//! cli support app services module.
//!
//! Contains cli support app services logic used by Berth command workflows.

use crate::config;

pub(crate) fn app_services(config: &config::Config) -> Vec<&config::ServiceConfig> {
    config
        .service
        .iter()
        .filter(|svc| svc.kind == config::Kind::App)
        .collect()
}
