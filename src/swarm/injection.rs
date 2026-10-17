//! swarm injection module.
//!
//! Contains swarm injection logic used by Berth command workflows.

use anyhow::Result;
use std::path::Path;

mod context;
mod resolver;
pub(crate) use resolver::endpoint_network;
mod values;

#[cfg(test)]
pub(crate) use values::resolve_injected_env_value;

/// Loads a swarm target's config (its default runtime environment).
pub(super) fn load_config_from_path(config_path: &Path) -> Result<crate::config::Config> {
    crate::config::load_config_with(crate::config::LoadConfigPathOptions::new(
        Some(config_path),
        None,
    ))
}

/// The source app as the consumer's runtime environment sees it: the same
/// environment of the source (`--env` reaches the sources too), except for a
/// test lease (`artisan test`, `testing-…`), which is the consumer's own and
/// sees the sources' default environment. Only names change; nothing is
/// provisioned for the source.
pub(super) fn source_in_runtime_env(
    source: &crate::config::ServiceConfig,
    runtime_env: Option<&str>,
) -> Result<crate::config::ServiceConfig> {
    match runtime_env.filter(|env| !is_test_lease_env(env)) {
        Some(env) => crate::config::service_in_runtime_env(source, env),
        None => Ok(source.clone()),
    }
}

/// Resolves project dependency injected env using configured inputs and runtime state.
pub(crate) fn resolve_project_dependency_injected_env(
    project_root: &Path,
    runtime_env: Option<&str>,
) -> Result<std::collections::HashMap<String, String>> {
    let Some(context) = resolve_workspace_swarm_context(project_root)? else {
        return Ok(std::collections::HashMap::new());
    };
    resolver::resolve_injected_env_from_swarm_context(&context, runtime_env)
}

/// The app services a project injects env from, with the endpoint network
/// each shares with its consumers (empty outside a swarm workspace).
///
/// # Errors
///
/// Returns an error when a source cannot be resolved.
pub(crate) fn resolve_project_dependency_endpoints(
    project_root: &Path,
    runtime_env: Option<&str>,
) -> Result<Vec<(crate::config::ServiceConfig, String)>> {
    let Some(context) = resolve_workspace_swarm_context(project_root)? else {
        return Ok(Vec::new());
    };
    resolver::resolve_dependency_endpoints_from_swarm_context(&context, runtime_env)
}

/// Resolves workspace swarm context using configured inputs and runtime state.
pub(super) fn resolve_workspace_swarm_context(
    project_root: &Path,
) -> Result<Option<context::ProjectSwarmContext>> {
    context::resolve_workspace_swarm_context(project_root)
}

/// Whether `env` is a name `artisan test` generates for its lease:
/// `testing-pool-{16 hex}-{slot}` or `testing-{8 hex}`. A user's own
/// `--env testing-eu` is an ordinary runtime environment.
#[must_use]
pub(crate) fn is_test_lease_env(env: &str) -> bool {
    let hex =
        |part: &str, len: usize| part.len() == len && part.bytes().all(|b| b.is_ascii_hexdigit());
    if let Some(rest) = env.strip_prefix("testing-pool-") {
        return rest.rsplit_once('-').is_some_and(|(key, slot)| {
            hex(key, 16) && !slot.is_empty() && slot.bytes().all(|b| b.is_ascii_digit())
        });
    }
    env.strip_prefix("testing-").is_some_and(|run| hex(run, 8))
}

#[cfg(test)]
mod lease_name_tests {
    use super::is_test_lease_env;

    #[test]
    fn only_generated_lease_names_are_test_leases() {
        assert!(is_test_lease_env("testing-pool-72d34fb23814d2bd-3"));
        assert!(is_test_lease_env("testing-0a1b2c3d"));
        assert!(!is_test_lease_env("testing-eu"));
        assert!(!is_test_lease_env("testing-pool-eu-1"));
        assert!(!is_test_lease_env("staging"));
    }
}
