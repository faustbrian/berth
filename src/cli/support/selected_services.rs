//! cli support selected services module.
//!
//! Contains cli support selected services logic used by Berth command workflows.

use anyhow::Result;
use std::collections::HashSet;

use crate::config;

use super::filter_services::filter_services;
use super::matches_filter::matches_filter;
use super::resolve_profile_targets;

pub(crate) fn selected_services<'a>(
    config: &'a config::Config,
    service: Option<&'a str>,
    kind: Option<config::Kind>,
    driver: Option<config::Driver>,
) -> Result<Vec<&'a config::ServiceConfig>> {
    selected_services_with_filters(config, service, &[], kind, driver, None)
}

pub(crate) fn selected_services_with_filters<'a>(
    config: &'a config::Config,
    service: Option<&'a str>,
    services: &[String],
    kind: Option<config::Kind>,
    driver: Option<config::Driver>,
    profile: Option<&'a str>,
) -> Result<Vec<&'a config::ServiceConfig>> {
    if let Some(profile_name) = profile {
        let targets = resolve_profile_targets(config, profile_name)?;
        return Ok(targets
            .into_iter()
            .filter(|svc| matches_filter(svc, kind, driver))
            .collect());
    }

    if !services.is_empty() {
        let mut selected = Vec::new();
        let mut seen = HashSet::new();
        for name in services {
            let svc = config::find_service(config, name)?;
            if !matches_filter(svc, kind, driver) {
                continue;
            }
            if seen.insert(svc.name.clone()) {
                selected.push(svc);
            }
        }
        return Ok(selected);
    }

    if let Some(name) = service {
        let svc = config::find_service(config, name)?;
        if matches_filter(svc, kind, driver) {
            return Ok(vec![svc]);
        }
        return Ok(Vec::new());
    }

    Ok(filter_services(&config.service, kind, driver))
}
