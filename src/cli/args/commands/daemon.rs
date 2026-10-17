//! cli args commands daemon module.
//!
//! Contains cli args for `berth daemon` workflows.

mod service;

use clap::{Args, Subcommand};
use std::path::PathBuf;

pub(crate) use service::{
    DaemonServiceArgs, DaemonServiceCommands, DaemonServiceInstallArgs, DaemonServicePrintArgs,
};

#[derive(Args)]
pub(crate) struct DaemonWatchPolicyArgs {
    /// Directory subtree to exclude from project discovery
    #[arg(long, value_name = "DIR")]
    pub(crate) exclude_dir: Vec<PathBuf>,
    /// Maximum number of projects to auto-start from one watch pass
    #[arg(long, value_name = "N")]
    pub(crate) max_projects: Option<usize>,
}

#[derive(Args)]
pub(crate) struct DaemonArgs {
    #[command(subcommand)]
    pub(crate) command: DaemonCommands,
}

#[derive(Subcommand)]
pub(crate) enum DaemonCommands {
    /// Start a per-project Berth daemon
    Start(DaemonStartArgs),
    /// Watch directories for Berth projects and ensure daemons are running
    Watch(DaemonWatchArgs),
    /// Install or inspect a login-time daemon watch service
    Service(DaemonServiceArgs),
    /// Show daemon status for a project
    Status(DaemonStatusArgs),
    /// Stop a per-project Berth daemon
    Stop(DaemonStopArgs),
    /// Show daemon log output for a project
    Logs(DaemonLogsArgs),
    #[command(hide = true)]
    Run(DaemonRunArgs),
}

#[derive(Args)]
pub(crate) struct DaemonStartArgs {
    /// Project directory or nested path inside a Berth project
    #[arg(long, value_name = "DIR")]
    pub(crate) path: PathBuf,
}

#[derive(Args)]
pub(crate) struct DaemonStatusArgs {
    /// Project directory or nested path inside a Berth project
    #[arg(long, value_name = "DIR")]
    pub(crate) path: PathBuf,
}

#[derive(Args)]
pub(crate) struct DaemonWatchArgs {
    /// Directory to scan for Berth projects
    #[arg(long, value_name = "DIR", required = true)]
    pub(crate) dir: Vec<PathBuf>,
    #[command(flatten)]
    pub(crate) policy: DaemonWatchPolicyArgs,
    /// Run one discovery pass and exit
    #[arg(long, default_value_t = false)]
    pub(crate) once: bool,
    /// Seconds between discovery scans
    #[arg(long, default_value_t = 30)]
    pub(crate) interval: u64,
}

#[derive(Args)]
pub(crate) struct DaemonStopArgs {
    /// Project directory or nested path inside a Berth project
    #[arg(long, value_name = "DIR")]
    pub(crate) path: PathBuf,
}

#[derive(Args)]
pub(crate) struct DaemonLogsArgs {
    /// Project directory or nested path inside a Berth project
    #[arg(long, value_name = "DIR")]
    pub(crate) path: PathBuf,
}

#[derive(Args)]
pub(crate) struct DaemonRunArgs {
    /// Project directory or nested path inside a Berth project
    #[arg(long, value_name = "DIR")]
    pub(crate) path: PathBuf,
}
