//! swarm injection module.
//!
//! Contains swarm injection logic used by Berth command workflows.

use anyhow::Result;
use std::path::Path;

mod context;
mod resolver;
mod values;

#[cfg(test)]
pub(crate) use values::resolve_injected_env_value;

pub(super) fn load_config_from_path(config_path: &Path) -> Result<crate::config::Config> {
    crate::config::load_config_with(crate::config::LoadConfigPathOptions::new(
        Some(config_path),
        None,
    ))
}

/// Resolves project dependency injected env using configured inputs and runtime state.
pub(crate) fn resolve_project_dependency_injected_env(
    project_root: &Path,
) -> Result<std::collections::HashMap<String, String>> {
    let Some(context) = resolve_workspace_swarm_context(project_root)? else {
        return Ok(std::collections::HashMap::new());
    };
    resolver::resolve_injected_env_from_swarm_context(&context)
}

/// Resolves workspace swarm context using configured inputs and runtime state.
pub(super) fn resolve_workspace_swarm_context(
    project_root: &Path,
) -> Result<Option<context::ProjectSwarmContext>> {
    context::resolve_workspace_swarm_context(project_root)
}
