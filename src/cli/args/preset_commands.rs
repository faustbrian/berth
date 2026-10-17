//! cli args preset commands module.
//!
//! Contains cli args preset commands logic used by Berth command workflows.

use clap::Subcommand;

#[derive(Subcommand)]
pub(crate) enum PresetCommands {
    /// List all available preset names
    List,
    /// Show resolved default values for one preset
    Show {
        #[arg()]
        name: String,
        #[arg(long, default_value = "toml")]
        format: String,
    },
}
