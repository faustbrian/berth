//! cli args commands lifecycle startup module.
//!
//! Contains cli args commands lifecycle startup logic used by Berth command workflows.

use clap::Args;

use crate::config;

use super::super::super::{PortStrategyArg, PullPolicyArg, default_parallelism};

#[derive(Args)]
pub(crate) struct SetupArgs {
    #[arg(long)]
    pub(crate) service: Option<String>,
    #[arg(long, value_enum)]
    pub(crate) kind: Option<config::Kind>,
    #[arg(long, default_value_t = 30)]
    pub(crate) timeout: u64,
    #[arg(long, default_value_t = default_parallelism())]
    pub(crate) parallel: usize,
}

impl SetupArgs {
    pub(crate) fn service(&self) -> Option<&str> {
        self.service.as_deref()
    }

    pub(crate) const fn kind(&self) -> Option<config::Kind> {
        self.kind
    }
}

#[derive(Args)]
pub(crate) struct StartArgs {
    #[arg(long)]
    pub(crate) service: Option<String>,
    #[arg(long, value_enum)]
    pub(crate) kind: Option<config::Kind>,
    /// Start a named service profile (full, infra, data, app, web, api)
    #[arg(long, conflicts_with_all = ["service", "kind"])]
    pub(crate) profile: Option<String>,
    /// Skip health waits during startup (default behavior for `start`)
    #[arg(long, default_value_t = false, conflicts_with = "wait")]
    pub(crate) no_wait: bool,
    /// Wait for service(s) to accept connections
    #[arg(long, default_value_t = false)]
    pub(crate) wait: bool,
    /// Timeout in seconds for readiness checks
    #[arg(long, default_value_t = 30)]
    pub(crate) wait_timeout: u64,
    /// Pull image before start
    #[arg(long, value_enum, default_value_t = PullPolicyArg::Missing)]
    pub(crate) pull: PullPolicyArg,
    /// Recreate containers even if their configuration and image haven't changed
    #[arg(long, default_value_t = false)]
    pub(crate) force_recreate: bool,
    /// Disable automatic browser launch + URL summary
    #[arg(long, default_value_t = false)]
    pub(crate) no_open: bool,
    /// Override app health path shown by `open`
    #[arg(long)]
    pub(crate) health_path: Option<String>,
    /// Skip starting workspace swarm dependencies
    #[arg(long, default_value_t = false)]
    pub(crate) no_deps: bool,
    /// Number of services to process in parallel
    #[arg(long, default_value_t = default_parallelism())]
    pub(crate) parallel: usize,
}

impl StartArgs {
    pub(crate) fn service(&self) -> Option<&str> {
        self.service.as_deref()
    }

    pub(crate) const fn kind(&self) -> Option<config::Kind> {
        self.kind
    }

    pub(crate) fn profile(&self) -> Option<&str> {
        self.profile.as_deref()
    }

    pub(crate) fn health_path(&self) -> Option<&str> {
        self.health_path.as_deref()
    }
}

#[derive(Args)]
pub(crate) struct UpArgs {
    #[arg(long)]
    pub(crate) service: Option<String>,
    #[arg(long, value_enum)]
    pub(crate) kind: Option<config::Kind>,
    /// Start a named service profile (full, infra, data, app, web, api)
    #[arg(long, conflicts_with_all = ["service", "kind"])]
    pub(crate) profile: Option<String>,
    /// Wait for service(s) to accept connections (enabled by default for `up`)
    #[arg(long, default_value_t = false)]
    pub(crate) wait: bool,
    /// Skip health waits during startup
    #[arg(long, default_value_t = false, conflicts_with = "wait")]
    pub(crate) no_wait: bool,
    /// Timeout in seconds for --wait
    #[arg(long, default_value_t = 30)]
    pub(crate) wait_timeout: u64,
    /// Pull image before start
    #[arg(long, value_enum, default_value_t = PullPolicyArg::Missing)]
    pub(crate) pull: PullPolicyArg,
    /// Recreate containers even if their configuration and image haven't changed
    #[arg(long, default_value_t = false)]
    pub(crate) force_recreate: bool,
    /// Publish all exposed ports to random host ports (enabled by default for `up`)
    #[arg(long, short = 'P', default_value_t = false)]
    pub(crate) publish_all: bool,
    /// Keep configured host ports instead of forcing random published ports
    #[arg(long, default_value_t = false, conflicts_with = "publish_all")]
    pub(crate) no_publish_all: bool,
    /// Strategy for assigning ports when randomization is enabled
    #[arg(long, value_enum, default_value_t = PortStrategyArg::Random)]
    pub(crate) port_strategy: PortStrategyArg,
    /// Seed used by `--port-strategy stable`
    #[arg(long)]
    pub(crate) port_seed: Option<String>,
    /// Persist random port assignments into `.berth.toml`
    #[arg(long, default_value_t = false, requires = "publish_all")]
    pub(crate) save_ports: bool,
    /// Write inferred service vars to local `.env`
    #[arg(long, default_value_t = false)]
    pub(crate) env_output: bool,
    /// Skip starting workspace swarm dependencies
    #[arg(long, default_value_t = false)]
    pub(crate) no_deps: bool,
    /// Apply configured data seed files after startup
    #[arg(long, default_value_t = false)]
    pub(crate) seed: bool,
    #[arg(long, default_value_t = default_parallelism())]
    pub(crate) parallel: usize,
}

impl UpArgs {
    pub(crate) fn service(&self) -> Option<&str> {
        self.service.as_deref()
    }

    pub(crate) const fn kind(&self) -> Option<config::Kind> {
        self.kind
    }

    pub(crate) fn profile(&self) -> Option<&str> {
        self.profile.as_deref()
    }

    pub(crate) fn port_seed(&self) -> Option<&str> {
        self.port_seed.as_deref()
    }
}

#[derive(Args)]
pub(crate) struct ApplyArgs {
    /// Skip starting workspace swarm dependencies
    #[arg(long, default_value_t = false)]
    pub(crate) no_deps: bool,
}

#[derive(Args)]
pub(crate) struct UpdateArgs {
    #[arg(long)]
    pub(crate) service: Option<String>,
    #[arg(long, value_enum)]
    pub(crate) kind: Option<config::Kind>,
    /// Update a named service profile (full, infra, data, app, web, api)
    #[arg(long, conflicts_with_all = ["service", "kind"])]
    pub(crate) profile: Option<String>,
    #[arg(long, default_value_t = false)]
    pub(crate) force_recreate: bool,
    /// Do not build images before starting containers
    #[arg(long, default_value_t = false)]
    pub(crate) no_build: bool,
    #[arg(long, default_value_t = false)]
    pub(crate) wait: bool,
    #[arg(long, default_value_t = 30)]
    pub(crate) wait_timeout: u64,
}

impl UpdateArgs {
    pub(crate) fn service(&self) -> Option<&str> {
        self.service.as_deref()
    }

    pub(crate) const fn kind(&self) -> Option<config::Kind> {
        self.kind
    }

    pub(crate) fn profile(&self) -> Option<&str> {
        self.profile.as_deref()
    }
}
