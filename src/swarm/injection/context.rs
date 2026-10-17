//! swarm injection context module.
//!
//! Contains swarm injection context logic used by Berth command workflows.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

use super::super::targets::resolve_swarm_root;

pub(crate) struct ProjectSwarmContext {
    pub(crate) workspace_root: PathBuf,
    pub(crate) workspace_config: crate::config::Config,
    pub(crate) target: crate::config::SwarmTarget,
}

/// Resolves workspace swarm context using configured inputs and runtime state.
pub(super) fn resolve_workspace_swarm_context(
    project_root: &Path,
) -> Result<Option<ProjectSwarmContext>> {
    let mut current = project_root;
    let project_root_canonical = std::fs::canonicalize(project_root).with_context(|| {
        format!(
            "failed to canonicalize project root {}",
            project_root.display()
        )
    })?;

    loop {
        if let Some(config_path) = crate::config::config_path_in_dir(current)? {
            let workspace_config = super::load_config_from_path(&config_path)?;
            if !workspace_config.swarm.is_empty() {
                let mut matched_target: Option<crate::config::SwarmTarget> = None;
                for target in &workspace_config.swarm {
                    let resolved = resolve_swarm_root(current, &target.root);
                    if let Ok(canonical_target_root) = std::fs::canonicalize(&resolved)
                        && canonical_target_root == project_root_canonical
                    {
                        matched_target = Some(target.clone());
                        break;
                    }
                }
                if let Some(target) = matched_target {
                    return Ok(Some(ProjectSwarmContext {
                        workspace_root: current.to_path_buf(),
                        workspace_config,
                        target,
                    }));
                }
            }
        }

        let Some(parent) = current.parent() else {
            break;
        };
        current = parent;
    }

    Ok(None)
}
