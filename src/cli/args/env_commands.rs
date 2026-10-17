//! cli args env commands module.
//!
//! Contains cli args env commands logic used by Berth command workflows.

use clap::Subcommand;
use std::path::PathBuf;

#[derive(Subcommand)]
pub(crate) enum EnvCommands {
    /// Write the managed environment to a new file (never an existing one)
    Generate {
        #[arg(long)]
        output: PathBuf,
        /// Write the host view (endpoints a host tool can reach) instead of
        /// what app containers receive
        #[arg(long, default_value_t = false)]
        host: bool,
    },
}
