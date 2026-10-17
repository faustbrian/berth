//! cli args config commands module.
//!
//! Contains cli args config commands logic used by Berth command workflows.

use clap::Subcommand;

#[derive(Subcommand)]
pub(crate) enum ConfigCommands {
    /// Convert helm, stackctl and .berth.toml project files into .berth.yaml
    Migrate,
}
