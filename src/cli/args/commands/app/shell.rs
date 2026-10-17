//! cli args commands app shell module.
//!
//! Contains cli args commands app shell logic used by Berth command workflows.

use clap::Args;

use crate::config;

use super::super::super::{PackageManagerArg, VersionManagerArg};

#[derive(Args)]
pub(crate) struct ExecArgs {
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
    /// Command and arguments to run
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    pub(crate) command: Vec<String>,
}

impl ExecArgs {
    pub(crate) fn service(&self) -> Option<&str> {
        self.service.as_deref()
    }

    pub(crate) fn profile(&self) -> Option<&str> {
        self.profile.as_deref()
    }
}

#[derive(Args)]
pub(crate) struct ArtisanArgs {
    #[arg(long)]
    pub(crate) service: Option<String>,
    #[arg(long, value_enum)]
    pub(crate) kind: Option<config::Kind>,
    /// Select a service profile (full, infra, data, app, web, api)
    #[arg(long, conflicts_with_all = ["service", "kind"])]
    pub(crate) profile: Option<String>,
    /// Enable Playwright bootstrap for browser-style artisan test runs
    #[arg(long, default_value_t = false)]
    pub(crate) browser: bool,
    #[arg(long, default_value_t = true, conflicts_with = "no_tty")]
    pub(crate) tty: bool,
    #[arg(long, default_value_t = false)]
    pub(crate) no_tty: bool,
    /// Artisan command and arguments
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    pub(crate) command: Vec<String>,
}

impl ArtisanArgs {
    pub(crate) fn service(&self) -> Option<&str> {
        self.service.as_deref()
    }

    pub(crate) fn profile(&self) -> Option<&str> {
        self.profile.as_deref()
    }
}

#[derive(Args)]
pub(crate) struct ComposerArgs {
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
    /// Composer command and arguments
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    pub(crate) command: Vec<String>,
}

impl ComposerArgs {
    pub(crate) fn service(&self) -> Option<&str> {
        self.service.as_deref()
    }

    pub(crate) fn profile(&self) -> Option<&str> {
        self.profile.as_deref()
    }
}

#[derive(Args)]
pub(crate) struct NodeArgs {
    #[arg(long)]
    pub(crate) service: Option<String>,
    #[arg(long, value_enum)]
    pub(crate) kind: Option<config::Kind>,
    /// Select a service profile (full, infra, data, app, web, api)
    #[arg(long, conflicts_with_all = ["service", "kind"])]
    pub(crate) profile: Option<String>,
    #[arg(long = "package-manager", value_enum)]
    pub(crate) package_manager: Option<PackageManagerArg>,
    #[arg(long = "version-manager", value_enum)]
    pub(crate) version_manager: Option<VersionManagerArg>,
    #[arg(long = "node-version")]
    pub(crate) node_version: Option<String>,
    #[arg(long, default_value_t = true, conflicts_with = "no_tty")]
    pub(crate) tty: bool,
    #[arg(long, default_value_t = false)]
    pub(crate) no_tty: bool,
    /// Package manager command and arguments
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    pub(crate) command: Vec<String>,
}

impl NodeArgs {
    pub(crate) fn service(&self) -> Option<&str> {
        self.service.as_deref()
    }

    pub(crate) fn profile(&self) -> Option<&str> {
        self.profile.as_deref()
    }
}

#[derive(Args)]
pub(crate) struct BunArgs {
    #[arg(long)]
    pub(crate) service: Option<String>,
    #[arg(long, value_enum)]
    pub(crate) kind: Option<config::Kind>,
    /// Select a service profile (full, infra, data, app, web, api)
    #[arg(long, conflicts_with_all = ["service", "kind"])]
    pub(crate) profile: Option<String>,
    #[arg(long = "bun-version")]
    pub(crate) bun_version: Option<String>,
    #[arg(long, default_value_t = true, conflicts_with = "no_tty")]
    pub(crate) tty: bool,
    #[arg(long, default_value_t = false)]
    pub(crate) no_tty: bool,
    /// Bun command and arguments
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    pub(crate) command: Vec<String>,
}

impl BunArgs {
    pub(crate) fn service(&self) -> Option<&str> {
        self.service.as_deref()
    }

    pub(crate) fn profile(&self) -> Option<&str> {
        self.profile.as_deref()
    }
}

#[derive(Args)]
pub(crate) struct DenoArgs {
    #[arg(long)]
    pub(crate) service: Option<String>,
    #[arg(long, value_enum)]
    pub(crate) kind: Option<config::Kind>,
    /// Select a service profile (full, infra, data, app, web, api)
    #[arg(long, conflicts_with_all = ["service", "kind"])]
    pub(crate) profile: Option<String>,
    #[arg(long = "deno-version")]
    pub(crate) deno_version: Option<String>,
    #[arg(long, default_value_t = true, conflicts_with = "no_tty")]
    pub(crate) tty: bool,
    #[arg(long, default_value_t = false)]
    pub(crate) no_tty: bool,
    /// Deno command and arguments
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    pub(crate) command: Vec<String>,
}

impl DenoArgs {
    pub(crate) fn service(&self) -> Option<&str> {
        self.service.as_deref()
    }

    pub(crate) fn profile(&self) -> Option<&str> {
        self.profile.as_deref()
    }
}
