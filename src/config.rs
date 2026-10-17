//! Configuration file parsing for berth.
//!
//! This module handles loading and parsing Berth configuration files.

#![allow(clippy::assigning_clones)] // Favor straightforward assignment in config mutation
#![allow(clippy::double_must_use)] // Public API methods intentionally signal important results
#![allow(clippy::match_same_arms)] // Duplicate arms keep driver/preset mapping explicit

mod api;
mod domain_names;
mod expansion;
mod paths;
mod presets;
mod raw;
mod runtime_env;
mod service_methods;
mod services;
#[cfg(test)]
mod tests;
mod types;
mod validation;

pub(crate) use api::load_raw_config_with;
pub use api::{
    LoadConfigPathOptions, LockfileDiff, MigrateConfigOptions, ProjectRootPathOptions,
    RawConfigPathOptions, SaveConfigPathOptions, apply_runtime_env, build_image_lock,
    config_path_in_dir, default_env_file_name, find_service, init_config, load_config,
    load_config_with, load_container_engine_with, load_lockfile_with, lockfile_diff,
    migrate_config_with, preferred_sql_client_flavor, preset_names, preset_preview, project_root,
    project_root_with, resolve_app_service, resolve_service, save_config_with, save_lockfile_with,
    update_service_host_port, update_service_port, verify_lockfile_with,
};
pub(crate) use raw::{RawConfig, RawServiceConfig};
pub(crate) use service_methods::network::{
    is_unspecified_port_allocation_host, normalize_host_for_port_allocation,
};
pub use types::{
    Config, ContainerEngine, DomainStrategy, Driver, HookOnError, HookPhase, HookRun, Kind,
    LockedImage, Lockfile, ProjectType, RestartPolicy, ServiceConfig, ServiceHook, SwarmGit,
    SwarmTarget,
};
