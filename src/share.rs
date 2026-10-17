//! Public share API surface.
//!
//! Provides tunnel lifecycle helpers for cloud sharing via supported providers.

use anyhow::Result;

use crate::config::{Config, ServiceConfig};

mod provider;
mod session;
mod state;
#[cfg(test)]
pub(crate) use state::with_test_state_home;
mod target;
#[cfg(test)]
mod tests;

pub use provider::ShareProvider;
pub use session::{ShareSession, ShareSessionStatus};

pub struct ShareStartResult {
    pub session: ShareSessionStatus,
    pub foreground_exit_code: Option<i32>,
}

/// Starts a share tunnel for a resolved app service.
///
/// # Errors
///
/// Returns an error if service resolution, state persistence, or process launch fails.
pub fn start(
    config: &Config,
    service: Option<&str>,
    provider: ShareProvider,
    detached: bool,
    timeout_secs: u64,
) -> Result<ShareStartResult> {
    let target = resolve_share_target(config, service)?;
    state::start_session(target, provider, detached, timeout_secs)
}

/// Returns share session statuses filtered by provider and/or service.
///
/// # Errors
///
/// Returns an error if state cannot be read.
pub fn status(
    service: Option<&str>,
    provider: Option<ShareProvider>,
) -> Result<Vec<ShareSessionStatus>> {
    state::status_sessions(service, provider)
}

/// Stops matching share sessions and removes them from tracked state.
///
/// # Errors
///
/// Returns an error if state cannot be read/written or process termination fails.
pub fn stop(
    service: Option<&str>,
    provider: Option<ShareProvider>,
    all: bool,
) -> Result<Vec<ShareSessionStatus>> {
    state::stop_sessions(service, provider, all)
}

fn resolve_share_target<'a>(
    config: &'a Config,
    service: Option<&str>,
) -> Result<&'a ServiceConfig> {
    crate::config::resolve_app_service(config, service)
}
