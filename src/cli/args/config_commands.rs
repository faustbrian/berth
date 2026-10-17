//! cli args config commands module.
//!
//! Contains cli args config commands logic used by Berth command workflows.

use clap::Subcommand;

#[derive(Subcommand)]
pub(crate) enum ConfigCommands {
    /// Convert helm, stackctl and .berth.toml project files into .berth.yaml
    Migrate,
    /// Check the project file without touching Docker: exits 0 when it is
    /// valid, 125 with the precise error otherwise
    Validate {
        /// Print one JSON document: the project, this checkout's instance and
        /// its services
        #[arg(long, default_value_t = false)]
        json: bool,
    },
}
