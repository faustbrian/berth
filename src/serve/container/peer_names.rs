//! Waits until an app container resolves the services it was given.

use anyhow::Result;
use std::collections::{BTreeSet, HashMap};
use std::time::{Duration, Instant};

use crate::config::{ContainerEngine, Driver, ServiceConfig};

/// How long an app may wait for its service names on Podman.
const WAIT: Duration = Duration::from_secs(30);

/// On Podman, waits until every service name the app was given as a host
/// (`DB_HOST=db`, `REDIS_HOST=cache`) that is already attached to its
/// network resolves inside its container, so its first command never meets
/// a name the DNS server does not answer yet. Services that start after the
/// app (listed after it, or not selected) are not waited for.
///
/// Rootless Podman 4.9 (CI, four parallel agents) once failed a test's first
/// Redis connection with `getaddrinfo for cache failed: Temporary failure in
/// name resolution` while `db` on the same network resolved; aardvark-dns
/// forwards a name it lacks upstream, which fails. Docker's embedded DNS
/// answers once `network connect` returns, so Docker needs no wait. Only PHP
/// app images (which ship `getent`) are probed.
///
/// # Errors
///
/// Returns an error naming every name that still does not resolve after
/// [`WAIT`].
pub(crate) fn wait_for_peer_names(
    target: &ServiceConfig,
    injected_env: &HashMap<String, String>,
) -> Result<()> {
    let probed = matches!(
        target.driver,
        Driver::Frankenphp | Driver::Horizon | Driver::Scheduler | Driver::Reverb
    );
    if crate::docker::container_engine() != ContainerEngine::Podman
        || crate::docker::is_dry_run()
        || !probed
    {
        return Ok(());
    }
    let Some(network) = target.runtime.network.as_deref() else {
        return Ok(());
    };
    let container = target.container_name()?;
    let attached = attached_names(network)?;
    let mut pending: BTreeSet<String> = injected_env
        .iter()
        .filter(|(key, _)| key.ends_with("_HOST"))
        .map(|(_, value)| value.trim().to_owned())
        .filter(|value| is_service_name(value) && attached.contains(value))
        .collect();
    let started = Instant::now();
    loop {
        let mut unresolved = BTreeSet::new();
        for name in pending {
            let output = crate::docker::run_docker_output(
                &["exec", &container, "getent", "hosts", &name],
                "failed to look up a service name",
            )?;
            match output.status.code() {
                Some(0) => {}
                // `getent`: the name has no entry (yet).
                Some(2) => {
                    unresolved.insert(name);
                }
                _ => anyhow::bail!(
                    "failed to look up {name} inside {container}: {}",
                    String::from_utf8_lossy(&output.stderr).trim()
                ),
            }
        }
        if unresolved.is_empty() {
            return Ok(());
        }
        if started.elapsed() >= WAIT {
            anyhow::bail!(
                "{} {} not resolve inside {container} after {}s (Podman's DNS server); run \
                 `berth up` again",
                unresolved.iter().cloned().collect::<Vec<_>>().join(", "),
                if unresolved.len() == 1 { "does" } else { "do" },
                WAIT.as_secs()
            );
        }
        pending = unresolved;
        std::thread::sleep(Duration::from_millis(250));
    }
}

/// The names (aliases) the containers attached to `network` answer to.
fn attached_names(network: &str) -> Result<BTreeSet<String>> {
    let filter = format!("network={network}");
    let listed = crate::docker::run_docker_output(
        &[
            "ps",
            "--filter",
            &filter,
            "--filter",
            "label=dev.berth.managed=true",
            "--format",
            "{{.Names}}",
        ],
        "failed to list a network's containers",
    )?;
    if !listed.status.success() {
        anyhow::bail!(
            "failed to list the containers on {network}: {}",
            String::from_utf8_lossy(&listed.stderr).trim()
        );
    }
    let mut names = BTreeSet::new();
    for container in String::from_utf8_lossy(&listed.stdout)
        .lines()
        .map(str::trim)
    {
        if container.is_empty() {
            continue;
        }
        let inspected = crate::docker::run_docker_output(
            &[
                "inspect",
                "--format",
                "{{json .NetworkSettings.Networks}}",
                container,
            ],
            "failed to inspect a container's networks",
        )?;
        if !inspected.status.success() {
            continue;
        }
        let networks: serde_json::Value = serde_json::from_slice(&inspected.stdout)
            .map_err(|error| anyhow::anyhow!("unexpected networks of {container}: {error}"))?;
        for alias in networks[network]["Aliases"]
            .as_array()
            .into_iter()
            .flatten()
        {
            if let Some(alias) = alias.as_str() {
                names.insert(alias.to_owned());
            }
        }
    }
    Ok(names)
}

/// A single DNS label, as service names are; not `localhost`.
fn is_service_name(value: &str) -> bool {
    !value.is_empty()
        && value != "localhost"
        && value.len() <= 63
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        && !value.starts_with('-')
        && !value.ends_with('-')
        && !value.bytes().all(|byte| byte.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::is_service_name;

    #[test]
    fn only_bare_service_names_are_probed() {
        for name in ["db", "cache", "search-2"] {
            assert!(is_service_name(name), "{name}");
        }
        for value in [
            "",
            "localhost",
            "127.0.0.1",
            "db.example.com",
            "-x",
            "x-",
            "8080",
            "Db",
        ] {
            assert!(!is_service_name(value), "{value}");
        }
    }
}
