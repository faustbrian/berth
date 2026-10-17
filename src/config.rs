//! Configuration file parsing for berth.
//!
//! This module handles loading and parsing Berth configuration files.

#![allow(clippy::assigning_clones)] // Favor straightforward assignment in config mutation
#![allow(clippy::double_must_use)] // Public API methods intentionally signal important results
#![allow(clippy::match_same_arms)] // Duplicate arms keep driver/preset mapping explicit

mod api;
mod domain_names;
mod expansion;
mod instance_naming;
mod paths;
mod presets;
pub(crate) mod project_file;
mod raw;
mod runtime_env;
mod service_methods;
mod services;
#[cfg(test)]
mod tests;
mod types;
mod unique_map;
mod validation;
pub(crate) mod yaml;

pub use api::{
    LoadConfigPathOptions, LockfileDiff, MigrateConfigOptions, ProjectRootPathOptions,
    RawConfigPathOptions, apply_runtime_env, build_image_lock, config_path_in_dir,
    default_env_file_name, find_service, init_config, is_default_runtime_env_name,
    load_config_with, load_container_engine_with, load_lockfile_with, lockfile_diff,
    preferred_sql_client_flavor, preset_names, preset_preview, project_root, project_root_with,
    resolve_app_service, resolve_service, save_lockfile_with, verify_lockfile_with,
};
pub(crate) use api::{load_raw_config_with, migrate_config_with};
pub(crate) use raw::{RawConfig, RawServiceConfig, RawSwarmTarget};
pub(crate) use service_methods::network::{
    is_unspecified_port_allocation_host, normalize_host_for_port_allocation,
};
#[cfg(test)]
pub(crate) use types::ServiceRuntime;
pub use types::{
    Config, ContainerEngine, DomainStrategy, Driver, HookOnError, HookPhase, HookRun, Isolation,
    Kind, LockedImage, Lockfile, ProjectType, RestartPolicy, ServiceConfig, ServiceHook,
    SharedEngine, SharedTarget, SwarmGit, SwarmTarget,
};
