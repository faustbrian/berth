//! config api services module.
//!
//! Contains config api services logic used by Berth command workflows.

use anyhow::Result;

use super::super::{Config, ServiceConfig, services};

/// Finds a service configuration by name.
///
/// # Errors
///
/// Returns an error if no service with the given name exists.
pub fn find_service<'a>(config: &'a Config, name: &str) -> Result<&'a ServiceConfig> {
    services::find_service(config, name)
}

/// Resolves a service by optional name.
///
/// # Errors
///
/// Returns an error for no services, ambiguous default, or unknown name.
pub fn resolve_service<'a>(config: &'a Config, name: Option<&str>) -> Result<&'a ServiceConfig> {
    services::resolve_service(config, name)
}

/// Resolves an app service by optional name.
///
/// # Errors
///
/// Returns an error for no app services, ambiguous default, or unknown name.
pub fn resolve_app_service<'a>(
    config: &'a Config,
    name: Option<&str>,
) -> Result<&'a ServiceConfig> {
    services::resolve_app_service(config, name)
}

/// Updates a configured service port by name.
///
/// # Errors
///
/// Returns an error if the service name cannot be found.
pub fn update_service_port(config: &mut Config, name: &str, port: u16) -> Result<()> {
    services::update_service_port(config, name, port)
}

/// Updates a configured service host and port by name.
///
/// Returns true when any value changed.
///
/// # Errors
///
/// Returns an error if the service name cannot be found.
pub fn update_service_host_port(
    config: &mut Config,
    name: &str,
    host: &str,
    port: u16,
) -> Result<bool> {
    services::update_service_host_port(config, name, host, port)
}
