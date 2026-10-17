//! cli args commands operations diagnostics module.
//!
//! Contains cli args commands operations diagnostics logic used by Berth command workflows.

use clap::Args;
use std::path::PathBuf;

use crate::cli::args::{EnvCommands, default_parallelism};
use crate::config;

#[derive(Args)]
pub(crate) struct AboutArgs {
    #[arg(long, default_value = "table")]
    pub(crate) format: String,
}

#[derive(Args)]
pub(crate) struct PsArgs {
    #[arg(long, default_value = "table")]
    pub(crate) format: String,
    #[arg(long, value_enum)]
    pub(crate) kind: Option<config::Kind>,
    #[arg(long, value_enum)]
    pub(crate) driver: Option<config::Driver>,
}

impl PsArgs {
    pub(crate) const fn kind(&self) -> Option<config::Kind> {
        self.kind
    }

    pub(crate) const fn driver(&self) -> Option<config::Driver> {
        self.driver
    }
}

#[derive(Args)]
pub(crate) struct HealthArgs {
    #[arg(long)]
    pub(crate) service: Vec<String>,
    #[arg(long, value_enum)]
    pub(crate) kind: Option<config::Kind>,
    /// Check a named service profile (full, infra, data, app, web, api)
    #[arg(long, conflicts_with_all = ["service", "kind"])]
    pub(crate) profile: Option<String>,
    #[arg(long, default_value = "table")]
    pub(crate) format: String,
    /// Timeout in seconds
    #[arg(long, default_value_t = 30)]
    pub(crate) timeout: u64,
    /// Seconds between health checks
    #[arg(long, default_value_t = 2)]
    pub(crate) interval: u64,
    /// Maximum check attempts before failing
    #[arg(long)]
    pub(crate) retries: Option<u32>,
    #[arg(long, default_value_t = default_parallelism())]
    pub(crate) parallel: usize,
}

impl HealthArgs {
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

#[derive(Args)]
pub(crate) struct EnvArgs {
    #[command(subcommand)]
    pub(crate) command: Option<EnvCommands>,
    #[arg(long)]
    pub(crate) service: Option<String>,
    /// Print the environment as JSON: `environment` (what app containers
    /// receive) and `host` (endpoints a host tool can reach)
    #[arg(long, default_value_t = false)]
    pub(crate) json: bool,
    // The 7.0.0 `.env` writers are kept only to fail with a precise message.
    #[arg(long, hide = true, value_enum)]
    pub(crate) kind: Option<config::Kind>,
    #[arg(long, hide = true)]
    pub(crate) env_file: Option<PathBuf>,
    #[arg(long, hide = true, default_value_t = false)]
    pub(crate) sync: bool,
    #[arg(long, hide = true, default_value_t = false)]
    pub(crate) purge: bool,
    #[arg(long, hide = true, default_value_t = false)]
    pub(crate) create_missing: bool,
}

impl EnvArgs {
    pub(crate) fn service(&self) -> Option<&str> {
        self.service.as_deref()
    }
}

#[derive(Args)]
pub(crate) struct LsArgs {
    #[arg(long, default_value = "table")]
    pub(crate) format: String,
    #[arg(long, value_enum)]
    pub(crate) kind: Option<config::Kind>,
    #[arg(long, value_enum)]
    pub(crate) driver: Option<config::Driver>,
}

impl LsArgs {
    pub(crate) const fn kind(&self) -> Option<config::Kind> {
        self.kind
    }

    pub(crate) const fn driver(&self) -> Option<config::Driver> {
        self.driver
    }
}
