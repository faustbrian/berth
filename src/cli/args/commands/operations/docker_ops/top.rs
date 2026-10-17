//! cli args commands operations docker ops top module.
//!
//! Contains top command args used by Berth command workflows.

use clap::Args;

use crate::config;

#[derive(Args)]
pub(crate) struct TopArgs {
    #[arg(long)]
    pub(crate) service: Vec<String>,
    #[arg(long, value_enum)]
    pub(crate) kind: Option<config::Kind>,
    /// Select a service profile (full, infra, data, app, web, api)
    #[arg(long, conflicts_with_all = ["service", "kind"])]
    pub(crate) profile: Option<String>,
    /// Optional ps args passed to `<engine> top`
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    pub(crate) args: Vec<String>,
}

impl TopArgs {
    pub(crate) fn service(&self) -> Option<&str> {
        self.service.first().map(String::as_str)
    }

    pub(crate) fn services(&self) -> &[String] {
        &self.service
    }

    pub(crate) const fn kind(&self) -> Option<config::Kind> {
        self.kind
    }

    pub(crate) fn profile(&self) -> Option<&str> {
        self.profile.as_deref()
    }
}
