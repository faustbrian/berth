//! cli args for `berth daemon service` workflows.

use clap::{Args, Subcommand};
use std::path::PathBuf;

use super::DaemonWatchPolicyArgs;

#[derive(Args)]
pub(crate) struct DaemonServiceArgs {
    #[command(subcommand)]
    pub(crate) command: DaemonServiceCommands,
}

#[derive(Subcommand)]
pub(crate) enum DaemonServiceCommands {
    /// Install and start a login-time daemon watch service
    Install(DaemonServiceInstallArgs),
    /// Show service installation state
    Status,
    /// Print the rendered service definition without installing it
    Print(DaemonServicePrintArgs),
    /// Stop and remove the installed service definition
    Uninstall,
}

#[derive(Args)]
pub(crate) struct DaemonServiceInstallArgs {
    /// Directory to scan for Berth projects
    #[arg(long, value_name = "DIR", required = true)]
    pub(crate) dir: Vec<PathBuf>,
    #[command(flatten)]
    pub(crate) policy: DaemonWatchPolicyArgs,
    /// Seconds between discovery scans
    #[arg(long, default_value_t = 30)]
    pub(crate) interval: u64,
}

#[derive(Args)]
pub(crate) struct DaemonServicePrintArgs {
    /// Directory to scan for Berth projects
    #[arg(long, value_name = "DIR", required = true)]
    pub(crate) dir: Vec<PathBuf>,
    #[command(flatten)]
    pub(crate) policy: DaemonWatchPolicyArgs,
    /// Seconds between discovery scans
    #[arg(long, default_value_t = 30)]
    pub(crate) interval: u64,
}
