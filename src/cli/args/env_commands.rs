//! cli args env commands module.
//!
//! Contains cli args env commands logic used by Berth command workflows.

use clap::Subcommand;
use std::path::PathBuf;

#[derive(Subcommand)]
pub(crate) enum EnvCommands {
    /// Generate a full env file from managed Berth app variables
    Generate {
        #[arg(long)]
        output: PathBuf,
    },
}
