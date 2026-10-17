//! cli args commands operations docker ops inspect module.
//!
//! Contains inspect command args used by Berth command workflows.

use clap::Args;

use crate::config;

#[derive(Args)]
pub(crate) struct InspectArgs {
    #[arg(long)]
    pub(crate) service: Vec<String>,
    #[arg(long, value_enum)]
    pub(crate) kind: Option<config::Kind>,
    /// Select a service profile (full, infra, data, app, web, api)
    #[arg(long, conflicts_with_all = ["service", "kind"])]
    pub(crate) profile: Option<String>,
    #[arg(long, conflicts_with = "json")]
    pub(crate) format: Option<String>,
    #[arg(long, default_value = "table", value_parser = ["table", "json"], ignore_case = true)]
    pub(crate) output: String,
    /// Emit structured JSON array output
    #[arg(long, default_value_t = false)]
    pub(crate) json: bool,
    #[arg(long, default_value_t = false)]
    pub(crate) size: bool,
    #[arg(long = "type")]
    pub(crate) object_type: Option<String>,
}

impl InspectArgs {
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

    pub(crate) fn object_type(&self) -> Option<&str> {
        self.object_type.as_deref()
    }

    pub(crate) fn profile(&self) -> Option<&str> {
        self.profile.as_deref()
    }
}
