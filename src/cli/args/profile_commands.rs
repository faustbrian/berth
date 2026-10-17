//! cli args profile commands module.
//!
//! Contains cli args profile commands logic used by Berth command workflows.

use clap::Subcommand;

#[derive(Subcommand)]
pub(crate) enum ProfileCommands {
    /// List built-in profile names
    List,
    /// Show services included by a profile
    Show {
        #[arg()]
        name: String,
        #[arg(long, default_value = "table")]
        format: String,
    },
}
