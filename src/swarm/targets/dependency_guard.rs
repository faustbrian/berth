//! swarm targets dependency guard module.
//!
//! Contains swarm targets dependency guard logic used by Berth command workflows.

use anyhow::Result;
use std::path::Path;

use super::ResolvedSwarmTarget;

pub(super) fn enforce_shared_down_dependency_guard(
    config: &crate::config::Config,
    only: &[String],
    expanded_targets: &[ResolvedSwarmTarget],
    force_down_deps: bool,
    workspace_root: &Path,
) -> Result<()> {
    if force_down_deps || only.is_empty() {
        return Ok(());
    }

    let explicit: std::collections::HashSet<&str> = only
        .iter()
        .map(String::as_str)
        .filter(|name| !name.trim().is_empty())
        .collect();
    let expanded: std::collections::HashSet<&str> = expanded_targets
        .iter()
        .map(|target| target.name.as_str())
        .collect();
    let implicit: std::collections::HashSet<&str> =
        expanded.difference(&explicit).copied().collect();

    if implicit.is_empty() {
        return Ok(());
    }

    let shared: Vec<String> = implicit
        .iter()
        .copied()
        .filter(|target| {
            config.swarm.iter().any(|candidate| {
                !expanded.contains(candidate.name.as_str())
                    && swarm_depends_on(candidate.name.as_str(), target, config)
            })
        })
        .map(ToOwned::to_owned)
        .collect();

    if shared.is_empty() {
        return Ok(());
    }

    anyhow::bail!(
        "refusing to down shared dependencies: {}. \
         Re-run with --force-down-deps or --no-deps. \
         workspace: {}",
        shared.join(", "),
        workspace_root.display()
    );
}

pub(super) fn swarm_depends_on(start: &str, target: &str, config: &crate::config::Config) -> bool {
    let deps_by_name: std::collections::HashMap<&str, &[String]> = config
        .swarm
        .iter()
        .map(|entry| (entry.name.as_str(), entry.depends_on.as_slice()))
        .collect();

    let mut stack = vec![start.to_owned()];
    let mut seen = std::collections::HashSet::new();

    while let Some(current) = stack.pop() {
        if !seen.insert(current.clone()) {
            continue;
        }
        let Some(deps) = deps_by_name.get(current.as_str()) else {
            continue;
        };
        if deps.iter().any(|dep| dep == target) {
            return true;
        }
        for dep in *deps {
            stack.push(dep.clone());
        }
    }

    false
}
