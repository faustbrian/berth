//! Swarm orchestration helpers for multi-project Berth workflows.

mod injection;
mod runner;
mod target_exec;
mod targets;

pub(crate) use injection::{
    endpoint_network, is_test_lease_env, resolve_project_dependency_endpoints,
    resolve_project_dependency_injected_env,
};
pub(crate) use runner::{
    RunProjectSwarmDependenciesOptions, RunSwarmOptions, run_project_swarm_dependencies, run_swarm,
};

#[cfg(test)]
pub(crate) use injection::resolve_injected_env_value;
#[cfg(test)]
pub(crate) use runner::swarm_child_args;
#[cfg(test)]
pub(crate) use targets::{
    ResolvedSwarmTarget, enforce_shared_down_dependency_guard, resolve_swarm_root,
    resolve_swarm_targets, swarm_depends_on,
};
