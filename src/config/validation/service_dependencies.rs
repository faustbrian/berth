//! Validates that every service `depends_on` entry names a service.

use anyhow::Result;

use crate::config::Config;

/// Fails on a `depends_on` entry that names no service of the project, at
/// load, instead of when `up` orders the services.
///
/// # Errors
///
/// Returns an error naming the service, the unknown entry and the services.
pub(super) fn validate_service_dependencies(config: &Config) -> Result<()> {
    for service in &config.service {
        for dependency in service.depends_on.iter().flatten() {
            if !config.service.iter().any(|other| &other.name == dependency) {
                let names: Vec<&str> = config.service.iter().map(|svc| svc.name.as_str()).collect();
                anyhow::bail!(
                    "service '{}' lists '{dependency}' in `depends_on`, but there is no such \
                     service (services: {}); fix `depends_on` of service '{}'",
                    service.name,
                    names.join(", "),
                    service.name
                );
            }
        }
    }
    Ok(())
}
