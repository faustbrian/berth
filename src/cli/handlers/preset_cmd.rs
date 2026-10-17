//! cli handlers preset cmd module.
//!
//! Contains cli handlers preset cmd logic used by Berth command workflows.

use anyhow::Result;

use crate::cli::handlers::serialize;
use crate::config;

/// Handles the `preset list` CLI command.
pub(crate) fn handle_preset_list() {
    for name in config::preset_names() {
        println!("{name}");
    }
}

/// Handles the `preset show` CLI command.
pub(crate) fn handle_preset_show(name: &str, format: &str) -> Result<()> {
    let service = config::preset_preview(name)?;
    serialize::print_pretty(&service, format)
}
