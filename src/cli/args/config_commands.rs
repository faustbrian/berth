//! cli args config commands module.
//!
//! Contains cli args config commands logic used by Berth command workflows.

use clap::Subcommand;

#[derive(Subcommand)]
pub(crate) enum ConfigCommands {
    /// Migrate local .berth.toml or .berth.yaml to the latest supported schema
    Migrate,
}
