//! cli args commands operations docker ops prune module.
//!
//! Contains prune command args used by Berth command workflows.

use clap::Args;

use crate::cli::args::default_parallelism;
use crate::config;

#[derive(Args)]
#[command(
    after_help = "Examples:\n  berth prune\n  berth prune --kind database\n  berth prune --all --force"
)]
pub(crate) struct PruneArgs {
    #[arg(long, conflicts_with = "all")]
    pub(crate) service: Option<String>,
    #[arg(long, value_enum, conflicts_with = "all")]
    pub(crate) kind: Option<config::Kind>,
    #[arg(long, default_value_t = default_parallelism(), conflicts_with = "all")]
    pub(crate) parallel: usize,
    /// Prune all stopped Docker containers (not only Berth-managed services),
    /// except another berth installation's
    #[arg(long, default_value_t = false)]
    pub(crate) all: bool,
    /// Required with --all; confirms intentional global prune
    #[arg(
        long,
        short,
        default_value_t = false,
        requires = "all",
        required_if_eq("all", "true")
    )]
    pub(crate) force: bool,
    /// Additional Docker prune filters (global mode only; `label=` and `until=` only)
    #[arg(long, requires = "all")]
    pub(crate) filter: Vec<String>,
}

impl PruneArgs {
    pub(crate) fn service(&self) -> Option<&str> {
        self.service.as_deref()
    }

    pub(crate) const fn kind(&self) -> Option<config::Kind> {
        self.kind
    }
}
