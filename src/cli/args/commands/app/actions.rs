//! cli args commands app actions module.
//!
//! Contains cli args commands app actions logic used by Berth command workflows.

use clap::Args;
use std::path::Path;
use std::path::PathBuf;

use crate::config;

#[derive(Args)]
pub(crate) struct AppCreateArgs {
    #[arg(long)]
    pub(crate) service: Option<String>,
    #[arg(long, default_value_t = false)]
    pub(crate) no_migrate: bool,
    #[arg(long, default_value_t = false)]
    pub(crate) seed: bool,
    #[arg(long, default_value_t = false)]
    pub(crate) no_storage_link: bool,
}

impl AppCreateArgs {
    pub(crate) fn service(&self) -> Option<&str> {
        self.service.as_deref()
    }
}

#[derive(Args)]
pub(crate) struct ServeArgs {
    /// App service name to serve
    #[arg(long)]
    pub(crate) service: Option<String>,
    #[arg(long, value_enum)]
    pub(crate) kind: Option<config::Kind>,
    /// Select a service profile (full, infra, data, app, web, api)
    #[arg(long, conflicts_with_all = ["service", "kind"])]
    pub(crate) profile: Option<String>,
    /// Recreate the app container before starting serve
    #[arg(long, default_value_t = false)]
    pub(crate) recreate: bool,
    /// Run foreground serve process in detached container mode
    #[arg(long, default_value_t = false)]
    pub(crate) detached: bool,
    /// Write inferred service vars to local `.env`
    #[arg(long, default_value_t = false)]
    pub(crate) env_output: bool,
    /// Trust the inner serve container CA in system trust store
    #[arg(long, default_value_t = false)]
    pub(crate) trust_container_ca: bool,
}

impl ServeArgs {
    pub(crate) fn service(&self) -> Option<&str> {
        self.service.as_deref()
    }

    pub(crate) fn profile(&self) -> Option<&str> {
        self.profile.as_deref()
    }
}

#[derive(Args)]
pub(crate) struct OpenArgs {
    /// App service name to inspect/open
    #[arg(long)]
    pub(crate) service: Option<String>,
    #[arg(long, value_enum)]
    pub(crate) kind: Option<config::Kind>,
    /// Select a service profile (full, infra, data, app, web, api)
    #[arg(long, conflicts_with_all = ["service", "kind", "all"])]
    pub(crate) profile: Option<String>,
    #[arg(long, default_value_t = false, conflicts_with = "service")]
    pub(crate) all: bool,
    #[arg(long)]
    pub(crate) health_path: Option<String>,
    #[arg(long, default_value_t = false)]
    pub(crate) no_browser: bool,
    /// Open database connection URL(s) via the platform opener
    #[arg(long, default_value_t = false)]
    pub(crate) database: bool,
    /// Print machine-readable JSON summary
    #[arg(long, default_value_t = false)]
    pub(crate) json: bool,
}

impl OpenArgs {
    pub(crate) fn service(&self) -> Option<&str> {
        self.service.as_deref()
    }

    pub(crate) fn health_path(&self) -> Option<&str> {
        self.health_path.as_deref()
    }

    pub(crate) fn profile(&self) -> Option<&str> {
        self.profile.as_deref()
    }
}

#[derive(Args)]
pub(crate) struct EnvScrubArgs {
    #[arg(long)]
    pub(crate) env_file: Option<PathBuf>,
}

impl EnvScrubArgs {
    pub(crate) fn env_file(&self) -> Option<&Path> {
        self.env_file.as_deref()
    }
}
