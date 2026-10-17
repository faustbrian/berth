//! cli args commands operations docker ops port module.
//!
//! Contains port command args used by Berth command workflows.

use clap::Args;

use crate::config;

#[derive(Args)]
pub(crate) struct PortArgs {
    #[arg(long)]
    pub(crate) service: Vec<String>,
    #[arg(long, value_enum)]
    pub(crate) kind: Option<config::Kind>,
    /// Select a service profile (full, infra, data, app, web, api)
    #[arg(long, conflicts_with_all = ["service", "kind"])]
    pub(crate) profile: Option<String>,
    #[arg(long, default_value = "table")]
    pub(crate) format: String,
    /// Emit structured JSON array output
    #[arg(long, default_value_t = false)]
    pub(crate) json: bool,
    /// Optional private port/protocol, for example `80/tcp`
    pub(crate) private_port: Option<String>,
}

impl PortArgs {
    pub(crate) fn service(&self) -> Option<&str> {
        self.service.first().map(String::as_str)
    }

    pub(crate) fn services(&self) -> &[String] {
        &self.service
    }

    pub(crate) const fn kind(&self) -> Option<config::Kind> {
        self.kind
    }

    pub(crate) fn private_port(&self) -> Option<&str> {
        self.private_port.as_deref()
    }

    pub(crate) fn profile(&self) -> Option<&str> {
        self.profile.as_deref()
    }
}
