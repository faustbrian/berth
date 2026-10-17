//! swarm targets resolver ordering module.
//!
//! Contains swarm targets resolver ordering logic used by Berth command workflows.

use anyhow::Result;
use std::collections::HashMap;

pub(super) fn order_target_names(
    by_name: &HashMap<&str, &crate::config::SwarmTarget>,
    selected_roots: Vec<String>,
    include_deps: bool,
) -> Result<Vec<String>> {
    if !include_deps {
        return Ok(selected_roots);
    }

    crate::dependency_order::order_dependency_names(
        &selected_roots,
        |current| {
            let Some(target) = by_name.get(current).copied() else {
                anyhow::bail!("unknown swarm dependency target '{current}'");
            };
            Ok(target.depends_on.clone())
        },
        |current| format!("circular swarm dependency detected at '{current}'"),
    )
}
