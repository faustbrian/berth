//! swarm targets resolver module.
//!
//! Contains swarm targets resolver logic used by Berth command workflows.

use anyhow::Result;
use std::path::{Path, PathBuf};

use super::ResolvedSwarmTarget;
use ordering::order_target_names;
use selection::select_targets;

mod ordering;
mod selection;

/// Resolves swarm targets using configured inputs and runtime state.
pub(super) fn resolve_swarm_targets(
    config: &crate::config::Config,
    workspace_root: &Path,
    only: &[String],
    include_deps: bool,
) -> Result<Vec<ResolvedSwarmTarget>> {
    if config.swarm.is_empty() {
        anyhow::bail!(
            "{}/.berth.yaml lists no `swarm` targets; run `berth swarm` from the workspace that \
             lists them, or pass `--project-root <workspace>`",
            workspace_root.display()
        );
    }

    let (by_name, selected_roots) = select_targets(config, only)?;
    let ordered_names = order_target_names(&by_name, selected_roots, include_deps)?;

    let targets: Vec<ResolvedSwarmTarget> = ordered_names
        .iter()
        .map(|name| {
            let target = by_name
                .get(name.as_str())
                .copied()
                .ok_or_else(|| anyhow::anyhow!("swarm target '{name}' missing"))?;
            Ok(ResolvedSwarmTarget {
                name: target.name.clone(),
                root: resolve_swarm_root(workspace_root, &target.root),
            })
        })
        .collect::<Result<Vec<_>>>()?;

    Ok(targets)
}

/// Resolves swarm root using configured inputs and runtime state.
pub(super) fn resolve_swarm_root(workspace_root: &Path, swarm_root: &Path) -> PathBuf {
    crate::cli::support::resolve_workspace_path(workspace_root, swarm_root)
}
