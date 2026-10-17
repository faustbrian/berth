//! cli handlers health cmd module.
//!
//! Contains cli handlers health cmd logic used by Berth command workflows.

use anyhow::Result;
use serde::Serialize;
use std::sync::{Arc, Mutex};

use super::serialize;
use super::service_scope::selected_services_in_scope;
use crate::{config, docker};

pub(crate) struct HandleHealthOptions<'a> {
    pub(crate) service: Option<&'a str>,
    pub(crate) services: &'a [String],
    pub(crate) kind: Option<config::Kind>,
    pub(crate) profile: Option<&'a str>,
    pub(crate) format: &'a str,
    pub(crate) timeout: u64,
    pub(crate) interval: u64,
    pub(crate) retries: Option<u32>,
    pub(crate) parallel: usize,
}

#[derive(Clone, Serialize)]
struct HealthStatus {
    name: String,
    ok: bool,
    error: Option<String>,
}

pub(crate) fn handle_health(
    config: &config::Config,
    options: HandleHealthOptions<'_>,
) -> Result<()> {
    let selected = selected_services_in_scope(
        config,
        options.service,
        options.services,
        options.kind,
        options.profile,
    )?;
    // Inspection never starts the gateway or attaches it to networks.
    crate::gateway::probe_without_starting_gateway();
    let statuses = Arc::new(Mutex::new(Vec::with_capacity(selected.len())));
    let statuses_shared = Arc::clone(&statuses);
    crate::cli::support::run_selected_services(&selected, options.parallel, move |service| {
        let status = match docker::wait_until_healthy(
            service,
            options.timeout,
            options.interval,
            options.retries,
        ) {
            Ok(()) => HealthStatus {
                name: service.name.clone(),
                ok: true,
                error: None,
            },
            Err(err) => HealthStatus {
                name: service.name.clone(),
                ok: false,
                error: Some(err.to_string()),
            },
        };
        let mut guard = statuses_shared
            .lock()
            .map_err(|_| anyhow::anyhow!("failed to lock health results"))?;
        guard.push(status);
        Ok(())
    })?;
    let statuses = statuses
        .lock()
        .map_err(|_| anyhow::anyhow!("failed to lock health results"))?
        .clone();

    if options.format.eq_ignore_ascii_case("json") {
        serialize::print_json_pretty(&statuses)?;
    }

    if statuses.iter().any(|status| !status.ok) {
        anyhow::bail!("health checks failed")
    }

    Ok(())
}
