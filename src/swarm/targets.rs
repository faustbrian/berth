//! swarm targets module.
//!
//! Contains swarm targets logic used by Berth command workflows.

use anyhow::Result;
use std::path::{Path, PathBuf};

mod bootstrap;
mod dependency_guard;
mod resolver;

#[derive(Clone)]
pub(crate) struct ResolvedSwarmTarget {
    pub(crate) name: String,
    pub(crate) root: PathBuf,
}

/// Resolves swarm targets using configured inputs and runtime state.
pub(crate) fn resolve_swarm_targets(
    config: &crate::config::Config,
    workspace_root: &Path,
    only: &[String],
    include_deps: bool,
) -> Result<Vec<ResolvedSwarmTarget>> {
    resolver::resolve_swarm_targets(config, workspace_root, only, include_deps)
}

/// Resolves swarm root using configured inputs and runtime state.
pub(crate) fn resolve_swarm_root(workspace_root: &Path, swarm_root: &Path) -> PathBuf {
    resolver::resolve_swarm_root(workspace_root, swarm_root)
}

pub(crate) fn bootstrap_swarm_targets(
    config: &crate::config::Config,
    targets: &[ResolvedSwarmTarget],
    quiet: bool,
) -> Result<()> {
    bootstrap::bootstrap_swarm_targets(config, targets, quiet)
}

/// Ensures swarm target configs exist exists and is in the required state.
pub(crate) fn ensure_swarm_target_configs_exist(
    config: &crate::config::Config,
    targets: &[ResolvedSwarmTarget],
) -> Result<()> {
    bootstrap::ensure_target_configs_exist(config, targets)
}

pub(crate) fn enforce_shared_down_dependency_guard(
    config: &crate::config::Config,
    only: &[String],
    expanded_targets: &[ResolvedSwarmTarget],
    force_down_deps: bool,
    workspace_root: &Path,
) -> Result<()> {
    dependency_guard::enforce_shared_down_dependency_guard(
        config,
        only,
        expanded_targets,
        force_down_deps,
        workspace_root,
    )
}

#[cfg(test)]
pub(crate) fn swarm_depends_on(start: &str, target: &str, config: &crate::config::Config) -> bool {
    dependency_guard::swarm_depends_on(start, target, config)
}
