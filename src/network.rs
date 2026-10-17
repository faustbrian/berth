//! Per-instance Docker networks.
//!
//! Each instance (and each `--env` runtime namespace inside it) gets its own
//! bridge network, so instances never reach each other's containers and
//! services are addressed by their service name.

mod ensure;
mod remove;
mod run_args;
mod subnet;

pub(crate) use ensure::ensure_service_network;
pub(crate) use remove::remove_unused_networks;
pub(crate) use run_args::network_run_args;
