//! The machine-readable result of `berth up --json`.
//!
//! One JSON document on stdout, both on success and on failure, so an agent
//! parses exactly one value; berth's progress stays on stderr.

use anyhow::Result;
use std::path::Path;

use crate::config::{self, Kind};

/// Schema version of the `up --json` document (see docs/v9/agents.md).
const SCHEMA: u32 = 1;

/// Options that shape the report.
pub(crate) struct UpReportOptions<'a> {
    pub(crate) waited: bool,
    /// Services this `up` started and waited for (its selection).
    pub(crate) selected: Vec<String>,
    /// `--service`, when it names an app service.
    pub(crate) service: Option<&'a str>,
    pub(crate) config_path: Option<&'a Path>,
    pub(crate) project_root: Option<&'a Path>,
}

/// Builds the document for a converged instance.
///
/// # Errors
///
/// Returns an error when the environment cannot be resolved.
pub(crate) fn ready_report(
    config: &config::Config,
    options: &UpReportOptions<'_>,
) -> Result<serde_json::Value> {
    let app = report_app(config, options.service);
    let views = crate::cli::handlers::env_cmd::env_views(
        config,
        app,
        options.config_path,
        options.project_root,
    )?;
    let services: Vec<serde_json::Value> = config
        .service
        .iter()
        .map(|service| {
            let waited =
                options.waited && options.selected.iter().any(|name| name == &service.name);
            service_entry(service, waited)
        })
        .collect();
    let urls: Vec<String> = config
        .service
        .iter()
        .filter(|service| crate::gateway::has_gateway_routes(service))
        .flat_map(crate::gateway::route_urls)
        .collect();
    let mut document = identity(config);
    document["status"] = "ready".into();
    document["urls"] = urls.into();
    document["services"] = services.into();
    document["credentials"] = credentials(config);
    document["environment"] = serde_json::to_value(&views.container)?;
    document["host_environment"] = serde_json::to_value(&views.host)?;
    Ok(document)
}

/// The app whose environment the report shows: the selected app service,
/// else the routable Laravel app, else the first routable app, else the only
/// app. Workers, schedulers and Horizon are app services too, so "the app"
/// must be chosen, never assumed to be named `app`.
fn report_app<'a>(config: &'a config::Config, service: Option<&'a str>) -> Option<&'a str> {
    let apps: Vec<&config::ServiceConfig> = config
        .service
        .iter()
        .filter(|svc| svc.kind == Kind::App)
        .collect();
    if let Some(name) = service.filter(|name| apps.iter().any(|app| app.name == *name)) {
        return Some(name);
    }
    apps.iter()
        .find(|app| crate::env::is_laravel_app(app) && crate::gateway::is_routable(app))
        .or_else(|| apps.iter().find(|app| crate::gateway::is_routable(app)))
        .or_else(|| (apps.len() == 1).then(|| &apps[0]))
        .map(|app| app.name.as_str())
}

/// Builds the document for a failed `up`.
#[must_use]
pub(crate) fn failed_report(config: &config::Config, error: &anyhow::Error) -> serde_json::Value {
    let mut document = identity(config);
    document["status"] = "failed".into();
    document["error"] = crate::docker::redact_known_secrets(&format!("{error:#}")).into();
    document
}

fn identity(config: &config::Config) -> serde_json::Value {
    let runtime = config.service.first().map(|service| &service.runtime);
    serde_json::json!({
        "schema": SCHEMA,
        "project": runtime.and_then(|runtime| runtime.project.clone()),
        "instance": runtime.and_then(|runtime| runtime.instance.clone()),
        "root": runtime.and_then(|runtime| runtime.root.as_ref().map(|root| root.display().to_string())),
    })
}

fn service_entry(service: &config::ServiceConfig, waited: bool) -> serde_json::Value {
    let container = service.container_name().ok();
    let state = container
        .as_deref()
        .and_then(crate::docker::inspect_status)
        .unwrap_or_else(|| "missing".to_owned());
    // Healthy only for a service this run waited for and that runs now.
    let health = if !waited {
        "not_checked"
    } else if state != "running" {
        "not_running"
    } else if crate::cli::support::is_health_checked(service) {
        "healthy"
    } else {
        "unchecked"
    };
    serde_json::json!({
        "name": service.name,
        "kind": crate::docker::kind_label_value(service.kind),
        "driver": format!("{:?}", service.driver).to_lowercase(),
        "container": container,
        "state": state,
        "health": health,
        "endpoint": format!("{}:{}", service.name, service.resolved_container_port()),
        "url": crate::network::host_url(service),
        "published": crate::network::is_published(service),
    })
}

/// Credentials of services that have any, keyed by service name.
fn credentials(config: &config::Config) -> serde_json::Value {
    let mut map = serde_json::Map::new();
    for service in &config.service {
        if service.kind == Kind::App {
            continue;
        }
        let mut entry = serde_json::Map::new();
        for (key, value) in [
            ("database", &service.database),
            ("username", &service.username),
            ("password", &service.password),
            ("access_key", &service.access_key),
            ("secret_key", &service.secret_key),
            ("api_key", &service.api_key),
            ("bucket", &service.bucket),
        ] {
            if let Some(value) = value {
                entry.insert(key.to_owned(), value.clone().into());
            }
        }
        if !entry.is_empty() {
            map.insert(service.name.clone(), entry.into());
        }
    }
    map.into()
}
