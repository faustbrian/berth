//! cli args commands operations docker ops stats module.
//!
//! Contains stats command args used by Berth command workflows.

use clap::Args;

use crate::config;

#[derive(Args)]
pub(crate) struct StatsArgs {
    #[arg(long)]
    pub(crate) service: Vec<String>,
    #[arg(long, value_enum)]
    pub(crate) kind: Option<config::Kind>,
    /// Select a service profile (full, infra, data, app, web, api)
    #[arg(long, conflicts_with_all = ["service", "kind"])]
    pub(crate) profile: Option<String>,
    /// Disable streaming and show a single snapshot
    #[arg(long, default_value_t = false)]
    pub(crate) no_stream: bool,
    #[arg(long)]
    pub(crate) format: Option<String>,
}

impl StatsArgs {
    pub(crate) fn service(&self) -> Option<&str> {
        self.service.first().map(String::as_str)
    }

    pub(crate) fn services(&self) -> &[String] {
        &self.service
    }

    pub(crate) const fn kind(&self) -> Option<config::Kind> {
        self.kind
    }

    pub(crate) fn format(&self) -> Option<&str> {
        self.format.as_deref()
    }

    pub(crate) fn profile(&self) -> Option<&str> {
        self.profile.as_deref()
    }
}
