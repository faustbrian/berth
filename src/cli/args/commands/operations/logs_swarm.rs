//! cli args commands operations logs swarm module.
//!
//! Contains cli args commands operations logs swarm logic used by Berth command workflows.

use clap::Args;

use crate::cli::args::{PortStrategyArg, default_parallelism};
use crate::config;

#[derive(Args)]
pub(crate) struct LogsArgs {
    #[arg(long, conflicts_with = "all")]
    pub(crate) service: Vec<String>,
    #[arg(long, value_enum)]
    pub(crate) kind: Option<config::Kind>,
    /// Tail logs for a named service profile (full, infra, data, app, web, api)
    #[arg(long, conflicts_with_all = ["service", "kind", "all"])]
    pub(crate) profile: Option<String>,
    #[arg(long, default_value_t = false, conflicts_with = "service")]
    pub(crate) all: bool,
    /// Prefix each log line with service name
    #[arg(long, default_value_t = false)]
    pub(crate) prefix: bool,
    /// Follow log output
    #[arg(long, short, default_value_t = false)]
    pub(crate) follow: bool,
    #[arg(long)]
    pub(crate) tail: Option<u64>,
    #[arg(long)]
    pub(crate) since: Option<String>,
    #[arg(long)]
    pub(crate) until: Option<String>,
    #[arg(long, short = 't', default_value_t = false)]
    pub(crate) timestamps: bool,
    /// Tail local Caddy access logs instead of container logs
    #[arg(long, default_value_t = false)]
    pub(crate) access: bool,
}

impl LogsArgs {
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

impl SwarmArgs {
    pub(crate) fn port_seed(&self) -> Option<&str> {
        self.port_seed.as_deref()
    }
}

#[derive(Args)]
pub(crate) struct SwarmArgs {
    /// Restrict to a comma-delimited subset of swarm target names
    #[arg(long, value_delimiter = ',')]
    pub(crate) only: Vec<String>,
    /// Run only selected targets and skip swarm dependencies
    #[arg(long, default_value_t = false)]
    pub(crate) no_deps: bool,
    /// Allow downing dependencies also required by non-selected targets
    #[arg(long, short, default_value_t = false, conflicts_with = "no_deps")]
    pub(crate) force: bool,
    /// Number of projects to process in parallel
    #[arg(long, default_value_t = default_parallelism())]
    pub(crate) parallel: usize,
    /// Stop after first failure; in parallel mode remaining targets are cancelled
    #[arg(long, default_value_t = false)]
    pub(crate) fail_fast: bool,
    /// Strategy for assigning ports during `swarm up`
    #[arg(long, value_enum, default_value_t = PortStrategyArg::Random)]
    pub(crate) port_strategy: PortStrategyArg,
    /// Seed used by `--port-strategy stable`
    #[arg(long)]
    pub(crate) port_seed: Option<String>,
    /// Write inferred service vars to local `.env` during `swarm up`
    #[arg(long, default_value_t = false)]
    pub(crate) env_output: bool,
    /// Command to run, e.g. `up`, `down`, `ps --format json`
    #[arg(required = true, trailing_var_arg = true, allow_hyphen_values = true)]
    pub(crate) command: Vec<String>,
}
