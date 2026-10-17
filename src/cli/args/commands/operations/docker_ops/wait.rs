//! cli args commands operations docker ops wait module.
//!
//! Contains wait command args used by Berth command workflows.

use clap::Args;

use crate::cli::args::default_parallelism;
use crate::config;

#[derive(Args)]
pub(crate) struct WaitArgs {
    #[arg(long)]
    pub(crate) service: Vec<String>,
    #[arg(long, value_enum)]
    pub(crate) kind: Option<config::Kind>,
    /// Select a service profile (full, infra, data, app, web, api)
    #[arg(long, conflicts_with_all = ["service", "kind"])]
    pub(crate) profile: Option<String>,
    #[arg(long)]
    pub(crate) condition: Option<String>,
    #[arg(long, default_value_t = default_parallelism())]
    pub(crate) parallel: usize,
}

impl WaitArgs {
    pub(crate) fn service(&self) -> Option<&str> {
        self.service.first().map(String::as_str)
    }

    pub(crate) fn services(&self) -> &[String] {
        &self.service
    }

    pub(crate) const fn kind(&self) -> Option<config::Kind> {
        self.kind
    }

    pub(crate) fn condition(&self) -> Option<&str> {
        self.condition.as_deref()
    }

    pub(crate) fn profile(&self) -> Option<&str> {
        self.profile.as_deref()
    }
}
