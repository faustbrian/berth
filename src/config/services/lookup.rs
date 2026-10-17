//! config services lookup module.
//!
//! Contains config services lookup logic used by Berth command workflows.

use anyhow::Result;

use crate::config::{Config, ServiceConfig};

use super::available_service_names;

mod app;

pub(crate) use app::resolve_app_service;

pub(crate) fn find_service<'a>(config: &'a Config, name: &str) -> Result<&'a ServiceConfig> {
    config
        .service
        .iter()
        .find(|svc| svc.name == name)
        .ok_or_else(|| {
            let available = available_service_names(config);
            anyhow::anyhow!(
                "service '{}' not found. Available services: {}",
                name,
                if available.is_empty() {
                    "none".to_owned()
                } else {
                    available.join(", ")
                }
            )
        })
}

pub(crate) fn resolve_service<'a>(
    config: &'a Config,
    name: Option<&str>,
) -> Result<&'a ServiceConfig> {
    if let Some(n) = name {
        find_service(config, n)
    } else if config.service.len() == 1 {
        config
            .service
            .first()
            .ok_or_else(|| anyhow::anyhow!("service list unexpectedly empty"))
    } else if config.service.is_empty() {
        anyhow::bail!("no services configured")
    } else {
        let available = available_service_names(config);
        anyhow::bail!(
            "multiple services configured, specify one: {}",
            available.join(", ")
        )
    }
}
