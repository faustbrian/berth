use super::Cli;
use crate::cli::args::{Commands, PortStrategyArg};
use crate::config::{Config, SwarmTarget};
use crate::swarm::{
    ResolvedSwarmTarget, enforce_shared_down_dependency_guard, resolve_injected_env_value,
    resolve_project_dependency_injected_env, resolve_swarm_root, resolve_swarm_targets,
    run_project_swarm_dependencies, swarm_child_args, swarm_depends_on,
};
use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;

mod cli_flags;
mod dependency_guards;
mod dispatch;
mod injected_env;
mod swarm_targets;
mod usage_docs;
