//! cli args module.
//!
//! Contains cli args logic used by Berth command workflows.

use clap::Parser;
use std::path::Path;
use std::path::PathBuf;

use crate::config::ContainerEngine;

mod arg_enums;
mod commands;
mod config_commands;
mod env_commands;
mod lock_commands;
mod preset_commands;
mod profile_commands;

pub(crate) use crate::javascript::{
    PackageManager as PackageManagerArg, VersionManager as VersionManagerArg,
};
pub(crate) use arg_enums::{PortStrategyArg, PullPolicyArg, ShareProviderArg};
pub(crate) use commands::Commands;
pub(crate) use commands::ShareCommands;
pub(crate) use commands::ShareProviderSelectionArgs;
pub(crate) use commands::TaskCommands;
#[cfg(test)]
pub(crate) use commands::TaskDepsArgs;
pub(crate) use commands::TaskDepsCommands;
pub(crate) use commands::{
    DaemonArgs, DaemonCommands, DaemonLogsArgs, DaemonRunArgs, DaemonServiceArgs,
    DaemonServiceCommands, DaemonServiceInstallArgs, DaemonServicePrintArgs, DaemonStartArgs,
    DaemonStatusArgs, DaemonStopArgs, DaemonWatchArgs, DaemonWatchPolicyArgs,
};
pub(crate) use config_commands::ConfigCommands;
pub(crate) use env_commands::EnvCommands;
pub(crate) use lock_commands::LockCommands;
pub(crate) use preset_commands::PresetCommands;
pub(crate) use profile_commands::ProfileCommands;

/// Returns the default parallelism for CLI commands that fan out work.
#[must_use]
pub(crate) fn default_parallelism() -> usize {
    std::thread::available_parallelism()
        .map(std::num::NonZeroUsize::get)
        .unwrap_or(1)
        .min(4)
}

#[derive(Parser)]
#[command(
    name = "berth",
    about = "Disposable, isolated Docker environments for every checkout (built for agents)",
    version
)]
#[non_exhaustive]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub(crate) command: Commands,
    #[arg(global = true, long, short)]
    pub(crate) quiet: bool,
    #[arg(global = true, long)]
    pub(crate) no_color: bool,
    #[arg(global = true, long)]
    pub(crate) dry_run: bool,
    #[arg(
        global = true,
        long,
        value_name = "PATH",
        conflicts_with = "project_root"
    )]
    pub(crate) config: Option<PathBuf>,
    #[arg(global = true, long, value_name = "DIR")]
    pub(crate) project_root: Option<PathBuf>,
    /// Runtime environment namespace (for example: `test`)
    #[arg(global = true, long, value_name = "NAME")]
    pub(crate) env: Option<String>,
    /// Instance ID for this checkout's environment (default: derived from the project path)
    #[arg(global = true, long, value_name = "ID", env = "BERTH_INSTANCE")]
    pub(crate) instance: Option<String>,
    /// Container runtime engine (`docker` or `podman`)
    #[arg(global = true, long, value_name = "ENGINE", env = "BERTH_ENGINE")]
    pub(crate) engine: Option<ContainerEngine>,
    /// Max concurrent heavy Docker operations
    #[arg(global = true, long, value_name = "N")]
    pub(crate) docker_max_heavy_ops: Option<usize>,
    /// Max concurrent Docker build operations
    #[arg(global = true, long, value_name = "N")]
    pub(crate) docker_max_build_ops: Option<usize>,
    /// Retry attempts for transient Docker failures
    #[arg(global = true, long, value_name = "N")]
    pub(crate) docker_retry_budget: Option<u32>,
    /// Number of pooled runtimes for `berth artisan test`
    #[arg(global = true, long, value_name = "N")]
    pub(crate) test_runtime_pool_size: Option<usize>,
    /// Enable reproducible mode (deterministic behavior with lockfile checks)
    #[arg(global = true, long, default_value_t = false)]
    pub(crate) repro: bool,
    /// Disable interactive prompts and TTY-dependent behavior
    #[arg(global = true, long, default_value_t = false)]
    pub(crate) non_interactive: bool,
}

impl Cli {
    pub(crate) fn config_path(&self) -> Option<&Path> {
        self.config.as_deref()
    }

    pub(crate) fn project_root_path(&self) -> Option<&Path> {
        self.project_root.as_deref()
    }

    pub(crate) fn runtime_env(&self) -> Option<&str> {
        self.env.as_deref()
    }

    pub(crate) fn instance(&self) -> Option<&str> {
        self.instance.as_deref()
    }
}

#[cfg(test)]
mod tests;
