//! docker health module.
//!
//! Contains docker health logic used by Berth command workflows.

use anyhow::Result;

use crate::config::ServiceConfig;
use crate::output::{self, LogLevel, Persistence};

use super::is_dry_run;
use checks::check_service_health;

mod checks;
mod http;

/// A failed check that waiting cannot fix: it ends the wait.
#[derive(Debug)]
pub(crate) struct ReadinessFailure(pub(crate) String);

impl std::fmt::Display for ReadinessFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for ReadinessFailure {}

/// Waits until the service accepts connections. Without a `timeout` it waits
/// as long as the container runs: how long a start takes depends on how loaded
/// the machine is, so only a container that stops fails the wait.
pub fn wait_until_healthy(
    service: &ServiceConfig,
    timeout: Option<u64>,
    interval: u64,
    retries: Option<u32>,
) -> Result<()> {
    let container_name = service.container_name()?;

    if is_dry_run() {
        output::event(
            &service.name,
            LogLevel::Info,
            &format!(
                "[dry-run] Wait for service health (timeout={timeout:?} interval={interval}s \
                 retries={retries:?})"
            ),
            Persistence::Transient,
        );
        return Ok(());
    }

    let start = std::time::Instant::now();
    let sleep_secs = interval.max(1);

    ensure_running(service, &container_name)?;

    output::event(
        &service.name,
        LogLevel::Info,
        "Waiting for service to accept connections",
        Persistence::Persistent,
    );

    let mut attempts: u32 = 0;
    loop {
        attempts = attempts.saturating_add(1);
        if attempts > 1 {
            crate::parallel::ensure_no_sibling_failed()?;
            ensure_running(service, &container_name)?;
        }

        match check_service_health(service, &container_name) {
            Ok(true) => {
                output::event(
                    &service.name,
                    LogLevel::Success,
                    "Service is ready",
                    Persistence::Persistent,
                );
                return Ok(());
            }
            Err(err) => {
                if err
                    .downcast_ref::<crate::gateway::ProbeSetupError>()
                    .is_some()
                {
                    return Err(err);
                }
                if err.downcast_ref::<ReadinessFailure>().is_some() {
                    return Err(err.context(format!("service '{}' cannot start", service.name)));
                }
                if retries.is_some_and(|max| attempts >= max) {
                    anyhow::bail!(
                        "Failed health check for '{}' after {} retries: {}",
                        service.name,
                        attempts,
                        err
                    );
                }
            }
            Ok(false) => {}
        }

        if retries.is_some_and(|max| attempts >= max) {
            anyhow::bail!(
                "Failed health check for '{}' after {} retries",
                service.name,
                attempts
            );
        }

        if let Some(timeout) = timeout
            && start.elapsed() >= std::time::Duration::from_secs(timeout)
        {
            anyhow::bail!(
                "Timed out after {timeout}s waiting for '{}' to become ready",
                service.name
            );
        }

        std::thread::sleep(std::time::Duration::from_secs(sleep_secs));
    }
}

/// Fails when the container no longer runs: a stopped or restarting
/// container will not become ready by waiting. A failed `inspect` is no
/// answer, so the wait goes on.
fn ensure_running(service: &ServiceConfig, container_name: &str) -> Result<()> {
    match super::container_state(container_name) {
        super::ContainerState::Running | super::ContainerState::Unknown => Ok(()),
        super::ContainerState::NotRunning(status) => anyhow::bail!(
            "Container '{container_name}' is not running (status: {status}); see `berth logs {}`",
            service.name
        ),
        super::ContainerState::Missing => {
            anyhow::bail!("Container '{container_name}' does not exist")
        }
    }
}
