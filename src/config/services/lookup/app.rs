//! config services lookup app module.
//!
//! Contains config services lookup app logic used by Berth command workflows.

use anyhow::Result;

use crate::config::{Config, Kind, ServiceConfig};

pub(crate) fn resolve_app_service<'a>(
    config: &'a Config,
    name: Option<&str>,
) -> Result<&'a ServiceConfig> {
    let app_services: Vec<&ServiceConfig> = config
        .service
        .iter()
        .filter(|svc| svc.kind == Kind::App)
        .collect();

    if let Some(target_name) = name {
        return app_services
            .into_iter()
            .find(|target| target.name == target_name)
            .ok_or_else(|| anyhow::anyhow!("app service '{target_name}' not found"));
    }

    if app_services.len() == 1 {
        return app_services
            .first()
            .copied()
            .ok_or_else(|| anyhow::anyhow!("app service list unexpectedly empty"));
    }

    if let Some(default_app) = app_services
        .iter()
        .copied()
        .find(|target| target.name == "app")
    {
        return Ok(default_app);
    }

    if app_services.is_empty() {
        anyhow::bail!("no app services configured")
    }

    let available: Vec<&str> = app_services
        .iter()
        .map(|target| target.name.as_str())
        .collect();
    anyhow::bail!(
        "multiple app services configured, specify one with --service: {}",
        available.join(", ")
    )
}
