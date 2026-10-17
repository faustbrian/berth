//! cli args commands lifecycle access module.
//!
//! Contains cli args commands lifecycle access logic used by Berth command workflows.

use clap::Args;

use crate::config;

#[derive(Args)]
pub(crate) struct UrlArgs {
    #[arg(long)]
    pub(crate) service: Option<String>,
    #[arg(long, default_value = "table")]
    pub(crate) format: String,
    /// Print JSON (the same as `--format json`)
    #[arg(long, default_value_t = false)]
    pub(crate) json: bool,
    #[arg(long, value_enum)]
    pub(crate) kind: Option<config::Kind>,
    #[arg(long, value_enum)]
    pub(crate) driver: Option<config::Driver>,
}

impl UrlArgs {
    pub(crate) fn service(&self) -> Option<&str> {
        self.service.as_deref()
    }

    pub(crate) const fn kind(&self) -> Option<config::Kind> {
        self.kind
    }

    pub(crate) const fn driver(&self) -> Option<config::Driver> {
        self.driver
    }
}
