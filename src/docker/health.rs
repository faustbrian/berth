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

    ensure_running(service, &container_name, Waited::Not)?;

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
            ensure_running(service, &container_name, Waited::Yet)?;
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
                        "service '{}' failed its health check {} times: {err:#}; see why with {}",
                        service.name,
                        attempts,
                        crate::followup::logs(service)
                    );
                }
            }
            Ok(false) => {}
        }

        if retries.is_some_and(|max| attempts >= max) {
            anyhow::bail!(
                "service '{}' failed its health check {} times; see why with {}",
                service.name,
                attempts,
                crate::followup::logs(service)
            );
        }

        if let Some(timeout) = timeout
            && start.elapsed() >= std::time::Duration::from_secs(timeout)
        {
            anyhow::bail!(
                "service '{}' was not ready after the {timeout}s limit; see what it is doing with \
                 {}; if it is only slow, {}",
                service.name,
                crate::followup::logs(service),
                crate::followup::raise_limit()
            );
        }

        std::thread::sleep(std::time::Duration::from_secs(sleep_secs));
    }
}

/// Whether the wait had started when a check failed: the first check also
/// serves `berth health` on an instance that was never started, so only a
/// later one may blame something that happened meanwhile.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Waited {
    Not,
    Yet,
}

/// Fails when the container no longer runs: a stopped or restarting
/// container will not become ready by waiting. A failed `inspect` is no
/// answer, so the wait goes on.
fn ensure_running(service: &ServiceConfig, container_name: &str, waited: Waited) -> Result<()> {
    match super::container_state(container_name) {
        super::ContainerState::Running | super::ContainerState::Unknown => Ok(()),
        super::ContainerState::NotRunning(status) if waited == Waited::Not => anyhow::bail!(
            "service '{}' is not running (container {container_name} is {status}); {} shows why \
             it stopped; once fixed, start it with {}",
            service.name,
            crate::followup::logs(service),
            crate::followup::for_service(service, "up")
        ),
        super::ContainerState::Missing if waited == Waited::Not => anyhow::bail!(
            "service '{}' has no container {container_name} (not started yet, or removed by \
             `berth down`); start it with {}",
            service.name,
            crate::followup::for_service(service, "up")
        ),
        super::ContainerState::NotRunning(status) => anyhow::bail!(
            "service '{}' stopped while berth waited for it (container {container_name} is \
             {status}); see why with {}, fix it and run {} again",
            service.name,
            crate::followup::logs(service),
            crate::followup::for_service(service, "up")
        ),
        super::ContainerState::Missing => anyhow::bail!(
            "service '{}' has no container {container_name}: it was removed while berth waited \
             for it (another `berth down`?); run {} again",
            service.name,
            crate::followup::for_service(service, "up")
        ),
    }
}
