//! swarm injection resolver module.
//!
//! Contains swarm injection resolver logic used by Berth command workflows.

use anyhow::{Context, Result};

use super::super::targets::resolve_swarm_root;
use super::context::ProjectSwarmContext;
use super::values::resolve_injected_env_value;

/// Resolves injected env from swarm context using configured inputs and runtime state.
pub(super) fn resolve_injected_env_from_swarm_context(
    context: &ProjectSwarmContext,
    runtime_env: Option<&str>,
) -> Result<std::collections::HashMap<String, String>> {
    if context.target.inject_env.is_empty() {
        return Ok(std::collections::HashMap::new());
    }

    let by_name: std::collections::HashMap<&str, &crate::config::SwarmTarget> = context
        .workspace_config
        .swarm
        .iter()
        .map(|target| (target.name.as_str(), target))
        .collect();

    let mut loaded_dependency_configs: std::collections::HashMap<String, crate::config::Config> =
        std::collections::HashMap::new();
    let mut result = std::collections::HashMap::new();

    for inject in &context.target.inject_env {
        let target_name = inject.from.trim();
        let Some(source_target) = by_name.get(target_name).copied() else {
            anyhow::bail!(
                "workspace target '{}' injects env from unknown target '{}'",
                context.target.name,
                inject.from
            );
        };

        if !loaded_dependency_configs.contains_key(target_name) {
            let dependency_root = resolve_swarm_root(&context.workspace_root, &source_target.root);
            let dependency_config_path = crate::swarm::target_config_path(
                target_name,
                &dependency_root,
                source_target.git.as_ref(),
            )?;
            let dependency_config = super::load_config_from_path(&dependency_config_path)
                .with_context(|| {
                    format!(
                        "failed to load dependency config for target '{}' from {}",
                        target_name,
                        dependency_config_path.display()
                    )
                })?;
            loaded_dependency_configs.insert(target_name.to_owned(), dependency_config);
        }

        let dependency_config = loaded_dependency_configs.get(target_name).ok_or_else(|| {
            anyhow::anyhow!(
                "internal error: dependency config cache missing target '{}'",
                target_name
            )
        })?;
        let source_service =
            crate::config::resolve_app_service(dependency_config, inject.service.as_deref())
                .with_context(|| {
                    format!(
                        "cannot pick the app service of swarm target '{}' that `inject_env` {} \
                         reads from; set `service: <name>` on that `inject_env` entry in the \
                         workspace .berth.yaml",
                        target_name, inject.env
                    )
                })?;
        let source_service = super::source_in_runtime_env(source_service, runtime_env)?;
        let resolved_value = resolve_injected_env_value(inject.value.trim(), &source_service)
            .with_context(|| {
                format!(
                    "failed resolving inject value '{}' for env '{}' from target '{}'",
                    inject.value, inject.env, target_name
                )
            })?;

        result.insert(inject.env.trim().to_owned(), resolved_value);
    }

    Ok(result)
}

/// The app services the target injects env from, each with the endpoint
/// network a consumer joins to reach it (see [`endpoint_network`]).
///
/// # Errors
///
/// Returns an error when a source target or its app service cannot be
/// resolved.
pub(super) fn resolve_dependency_endpoints_from_swarm_context(
    context: &ProjectSwarmContext,
    runtime_env: Option<&str>,
) -> Result<Vec<(crate::config::ServiceConfig, String)>> {
    let mut endpoints: Vec<(crate::config::ServiceConfig, String)> = Vec::new();
    for inject in &context.target.inject_env {
        let target_name = inject.from.trim();
        let Some(source_target) = context
            .workspace_config
            .swarm
            .iter()
            .find(|target| target.name == target_name)
        else {
            anyhow::bail!(
                "workspace target '{}' injects env from unknown target '{}'",
                context.target.name,
                inject.from
            );
        };
        let dependency_root = resolve_swarm_root(&context.workspace_root, &source_target.root);
        let dependency_config_path = crate::swarm::target_config_path(
            target_name,
            &dependency_root,
            source_target.git.as_ref(),
        )?;
        let dependency_config = super::load_config_from_path(&dependency_config_path)?;
        let source_service =
            crate::config::resolve_app_service(&dependency_config, inject.service.as_deref())?;
        let source_service = super::source_in_runtime_env(source_service, runtime_env)?;
        let Some(network) = endpoint_network(&source_service) else {
            continue;
        };
        if !endpoints.iter().any(|(_, known)| *known == network) {
            endpoints.push((source_service.clone(), network));
        }
    }
    Ok(endpoints)
}

/// The network a source app shares with its consumers: `{instance
/// network}_ep`. Consumers never join the source's instance network, where
/// every service answers to its service name (`db`, `redis`): a consumer
/// would then resolve its own `db` to the source's database by Docker's
/// network-name order. On the endpoint network only container names (unique
/// per instance) resolve.
#[must_use]
pub(crate) fn endpoint_network(source: &crate::config::ServiceConfig) -> Option<String> {
    source
        .runtime
        .network
        .as_ref()
        .map(|network| format!("{network}_ep"))
}
