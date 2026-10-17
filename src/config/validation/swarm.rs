//! config validation swarm module.
//!
//! Contains config validation swarm logic used by Berth command workflows.

use anyhow::Result;
use std::collections::HashSet;

use crate::config::Config;

/// Validates swarm targets and reports actionable failures.
pub(super) fn validate_swarm_targets(config: &Config) -> Result<()> {
    if config.swarm.is_empty() {
        return Ok(());
    }

    let mut names = HashSet::new();

    for target in &config.swarm {
        let normalized = target.name.trim();
        if normalized.is_empty() {
            anyhow::bail!("swarm target name must not be empty");
        }
        if target.root.as_os_str().is_empty() {
            anyhow::bail!("swarm target '{normalized}' must define a non-empty root");
        }
        if !names.insert(normalized.to_owned()) {
            anyhow::bail!("duplicate swarm target name: '{normalized}'");
        }
    }

    for target in &config.swarm {
        let normalized = target.name.trim();
        for dependency in &target.depends_on {
            let dep = dependency.trim();
            if dep.is_empty() {
                anyhow::bail!("swarm target '{normalized}' has an empty dependency name");
            }
            if dep == normalized {
                anyhow::bail!("swarm target '{normalized}' cannot depend on itself");
            }
            if !names.contains(dep) {
                anyhow::bail!("swarm target '{normalized}' depends on unknown target '{dep}'");
            }
        }

        let mut env_names = HashSet::new();
        for inject in &target.inject_env {
            let env = inject.env.trim();
            if env.is_empty() {
                anyhow::bail!("swarm target '{normalized}' has an empty inject_env.env");
            }
            if !env_names.insert(env.to_owned()) {
                anyhow::bail!(
                    "swarm target '{normalized}' has duplicate inject env variable '{env}'"
                );
            }

            let from = inject.from.trim();
            if from.is_empty() {
                anyhow::bail!("swarm target '{normalized}' has an empty inject_env.from");
            }
            if !names.contains(from) {
                anyhow::bail!("swarm target '{normalized}' injects from unknown target '{from}'");
            }
            // Injected endpoints name the source's container on its network,
            // which the consumer joins, so the source must be up first.
            if !target
                .depends_on
                .iter()
                .any(|dependency| dependency.trim() == from)
            {
                anyhow::bail!(
                    "swarm target '{normalized}' injects env from '{from}' but does not depend on \
                     it; add '{from}' to its depends_on"
                );
            }

            let value = inject.value.trim();
            if value.is_empty() {
                anyhow::bail!("swarm target '{normalized}' has an empty inject_env.value");
            }
            if let Some(token) = value.strip_prefix(':')
                && !matches!(
                    token,
                    "domain" | "host" | "port" | "scheme" | "base_url" | "url"
                )
            {
                anyhow::bail!(
                    "swarm target '{normalized}' inject_env for '{env}' uses unsupported token '{value}'"
                );
            }
        }

        if let Some(git) = &target.git {
            if git.repo.trim().is_empty() {
                anyhow::bail!("swarm target '{normalized}' has an empty git.repo");
            }
            if let Some(branch) = &git.branch
                && branch.trim().is_empty()
            {
                anyhow::bail!("swarm target '{normalized}' has an empty git.branch");
            }
        }
    }

    Ok(())
}
