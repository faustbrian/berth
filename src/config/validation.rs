//! config validation module.
//!
//! Contains config validation logic used by Berth command workflows.

use anyhow::Result;

use super::Config;

mod container_names;
mod swarm;

/// Validates and resolves container names and reports actionable failures.
pub(super) fn validate_and_resolve_container_names(config: &mut Config) -> Result<()> {
    container_names::validate_and_resolve_container_names(config)
}

/// Validates swarm targets and reports actionable failures.
pub(super) fn validate_swarm_targets(config: &Config) -> Result<()> {
    swarm::validate_swarm_targets(config)
}
