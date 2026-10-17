//! swarm targets resolver selection module.
//!
//! Contains swarm targets resolver selection logic used by Berth command workflows.

use anyhow::Result;
use std::collections::HashMap;

pub(super) fn select_targets<'a>(
    config: &'a crate::config::Config,
    only: &[String],
) -> Result<(
    HashMap<&'a str, &'a crate::config::SwarmTarget>,
    Vec<String>,
)> {
    let by_name: HashMap<&str, &crate::config::SwarmTarget> = config
        .swarm
        .iter()
        .map(|target| (target.name.as_str(), target))
        .collect();
    let available_names: Vec<&str> = config
        .swarm
        .iter()
        .map(|target| target.name.as_str())
        .collect();

    let selected_roots: Vec<String> = if only.is_empty() {
        available_names
            .iter()
            .map(|name| (*name).to_owned())
            .collect()
    } else {
        let selected: Vec<&str> = only
            .iter()
            .map(|name| name.trim())
            .filter(|name| !name.is_empty())
            .collect();
        let unknown: Vec<&str> = selected
            .iter()
            .copied()
            .filter(|name| !by_name.contains_key(name))
            .collect();
        if !unknown.is_empty() {
            anyhow::bail!(
                "unknown swarm targets: {}. Available: {}",
                unknown.join(", "),
                available_names.join(", ")
            );
        }
        selected.into_iter().map(ToOwned::to_owned).collect()
    };

    if selected_roots.is_empty() {
        anyhow::bail!("no swarm targets selected");
    }

    Ok((by_name, selected_roots))
}
