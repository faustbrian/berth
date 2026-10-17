//! config api runtime env module.
//!
//! Contains config api runtime env logic used by Berth command workflows.

use anyhow::Result;

use super::super::{Config, runtime_env};

/// Applies runtime environment namespacing to container names and service ports.
///
/// This keeps parallel environments (for example local and test) isolated.
///
/// # Errors
///
/// Returns an error if the env label is invalid or if adjusted ports overflow.
pub fn apply_runtime_env(config: &mut Config, env_name: &str) -> Result<()> {
    runtime_env::apply_runtime_env(config, env_name)
}

/// Returns the default env file name for an optional runtime environment.
///
/// # Errors
///
/// Returns an error if the env label is invalid.
pub fn default_env_file_name(runtime_env: Option<&str>) -> Result<String> {
    runtime_env::default_env_file_name(runtime_env)
}
