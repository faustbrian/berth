//! cli args commands app php tool module.
//!
//! Contains shared php tool wrapper args used by Berth command workflows.

use clap::Args;

use crate::config;

#[derive(Args)]
pub(crate) struct PhpToolArgs {
    #[arg(long)]
    pub(crate) service: Option<String>,
    #[arg(long, value_enum)]
    pub(crate) kind: Option<config::Kind>,
    /// Select a service profile (full, infra, data, app, web, api)
    #[arg(long, conflicts_with_all = ["service", "kind"])]
    pub(crate) profile: Option<String>,
    #[arg(long, default_value_t = true, conflicts_with = "no_tty")]
    pub(crate) tty: bool,
    #[arg(long, default_value_t = false)]
    pub(crate) no_tty: bool,
    /// Tool command and arguments
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    pub(crate) command: Vec<String>,
}

impl PhpToolArgs {
    pub(crate) fn service(&self) -> Option<&str> {
        self.service.as_deref()
    }

    pub(crate) fn profile(&self) -> Option<&str> {
        self.profile.as_deref()
    }
}
