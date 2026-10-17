//! Publishes the complete route set to the gateway.

use anyhow::{Context, Result};
use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

use super::GatewayPorts;
use super::container::{GatewayDirs, GatewayState, ensure_gateway, gateway_container_name};
use super::render::{GatewayRoute, render_config};

/// Label carrying a container's route hosts (comma-separated).
pub(crate) const ROUTE_HOSTS_LABEL: &str = "dev.berth.route.hosts";
/// Label carrying the container port the routes forward to.
pub(crate) const ROUTE_PORT_LABEL: &str = "dev.berth.route.port";
/// Label carrying the container's instance network.
pub(crate) const NETWORK_LABEL: &str = "dev.berth.network";

/// How long a berth process waits for another one to finish publishing.
const PUBLISH_LOCK_TIMEOUT: Duration = Duration::from_secs(180);
/// How long a freshly started gateway may take to accept its admin API.
const START_TIMEOUT: Duration = Duration::from_secs(20);

/// A routed container as its labels describe it.
#[derive(Debug, Eq, PartialEq)]
pub(super) struct LabeledRoute {
    pub(super) container: String,
    pub(super) hosts: Vec<String>,
    pub(super) port: String,
    pub(super) network: String,
}

/// Why routes are being published.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PublishMode<'a> {
    /// A container (name, unique instance route) was started and claims its
    /// routes: the gateway is created if needed, and a collision of its
    /// instance route fails. Authored domains that collide are withheld with
    /// a warning; the instance stays reachable on its own route.
    Claim(&'a str, &'a str, u16),
    /// A container was removed: routes are withdrawn without ever creating
    /// or starting the gateway.
    Withdraw,
}

/// Rebuilds the gateway configuration from the labels of every running
/// routed container of this installation, attaches the gateway to their
/// networks (answering each network's routes as aliases) and applies it.
///
/// A host claimed by two containers is withheld from both, while every other
/// route keeps being published (8.x FI-53). A configuration the running
/// gateway rejects is never persisted, so a gateway restart always loads the
/// last accepted document (8.x FI-57).
///
/// # Errors
///
/// Returns an error when the claiming container's hosts collide, or the
/// gateway cannot be started, attached or reloaded.
pub(crate) fn publish_routes(mode: PublishMode<'_>) -> Result<()> {
    if crate::docker::is_dry_run() {
        crate::output::event(
            "gateway",
            crate::output::LogLevel::Info,
            "[dry-run] Publish gateway routes",
            crate::output::Persistence::Transient,
        );
        return Ok(());
    }
    let installation = crate::home::installation_id()?;
    let dirs = GatewayDirs::ensure()?;
    let _lock = crate::home::HostLock::acquire("gateway", PUBLISH_LOCK_TIMEOUT)?;
    // Read under the lock, so a concurrent first run's record is seen.
    let ports = GatewayPorts::current()?;

    let labeled = discover_routes(&installation)?;
    let plan = plan_routes(&labeled)?;
    let current = dirs.config.join("caddy.json");
    let candidate = dirs.config.join("caddy.next.json");
    ensure_start_document(&dirs, ports)?;
    let rendered = serde_json::to_string_pretty(&render_config(&plan.routes, ports))
        .context("failed to render gateway config")?;
    crate::config::yaml::write_file_atomically(&candidate, &rendered)?;

    let gateway = gateway_container_name(&installation);
    let running = match mode {
        PublishMode::Claim(_, _, baked_port) => {
            start_gateway(&installation, ports, &dirs)?;
            // A first run that loaded its config before another process
            // recorded different ports would serve URLs on the wrong port.
            if baked_port != ports.https {
                anyhow::bail!(
                    "this command resolved the gateway's HTTPS port as {baked_port}, but the \
                     gateway serves {}; rerun it",
                    ports.https
                );
            }
            true
        }
        PublishMode::Withdraw => super::container::is_running(&gateway)?,
    };
    if running {
        let attached = attached_aliases(&gateway)?;
        for (network, hosts) in &plan.aliases {
            attach(&gateway, network, hosts, attached.get(network))?;
        }
        if let Err(error) = reload(&gateway, "/etc/berth/caddy.next.json") {
            drop(std::fs::remove_file(&candidate));
            return Err(error);
        }
    }
    std::fs::rename(&candidate, &current)
        .with_context(|| format!("failed to install {}", current.display()))?;

    for (host, claimants) in &plan.conflicts {
        if let PublishMode::Claim(own, own_route, _) = mode
            && host == own_route
            && claimants.iter().any(|claimant| claimant == own)
        {
            anyhow::bail!(
                "route {host} is claimed by {}; use another --instance or bring the other \
                 down",
                claimants.join(" and ")
            );
        }
        crate::output::event(
            "gateway",
            crate::output::LogLevel::Warn,
            &format!(
                "Route {host} withheld: claimed by {}",
                claimants.join(" and ")
            ),
            crate::output::Persistence::Persistent,
        );
    }
    Ok(())
}

/// Fails before a container starts when another running container of this
/// installation already serves its unique instance route (possible only
/// when `-` in names makes two instance/project/service triples spell the
/// same label).
///
/// # Errors
///
/// Returns an error naming the host and its owner, or when Docker fails.
pub(crate) fn ensure_route_unclaimed(own_container: &str, route: &str) -> Result<()> {
    if crate::docker::is_dry_run() {
        return Ok(());
    }
    let hosts = [route];
    let installation = crate::home::installation_id()?;
    for entry in discover_routes(&installation)? {
        if entry.container == own_container {
            continue;
        }
        if let Some(host) = entry
            .hosts
            .iter()
            .find(|host| hosts.contains(&host.as_str()))
        {
            anyhow::bail!(
                "route {host} is already served by {}; use another --instance or bring {} \
                 down first",
                entry.container,
                entry.container
            );
        }
    }
    Ok(())
}

/// Routes, per-network aliases and withheld hosts derived from the labels.
#[derive(Debug, Default)]
pub(super) struct RoutePlan {
    pub(super) routes: Vec<GatewayRoute>,
    pub(super) aliases: BTreeMap<String, BTreeSet<String>>,
    pub(super) conflicts: BTreeMap<String, Vec<String>>,
}

/// Turns labeled containers into routes and per-network gateway aliases.
/// Every claimant of a host claimed more than once is excluded for that host.
pub(super) fn plan_routes(labeled: &[LabeledRoute]) -> Result<RoutePlan> {
    let mut claimants: BTreeMap<&str, Vec<&LabeledRoute>> = BTreeMap::new();
    for entry in labeled {
        if entry.port.parse::<u16>().is_err() {
            anyhow::bail!(
                "container {} has an invalid {ROUTE_PORT_LABEL} label '{}'",
                entry.container,
                entry.port
            );
        }
        for host in &entry.hosts {
            claimants.entry(host).or_default().push(entry);
        }
    }
    let mut plan = RoutePlan::default();
    for (host, entries) in claimants {
        if let [entry] = entries.as_slice() {
            plan.routes.push(GatewayRoute {
                host: host.to_owned(),
                upstream: format!("{}:{}", entry.container, entry.port),
            });
            if !entry.network.is_empty() {
                plan.aliases
                    .entry(entry.network.clone())
                    .or_default()
                    .insert(host.to_owned());
            }
        } else {
            let mut names: Vec<String> = entries
                .iter()
                .map(|entry| entry.container.clone())
                .collect();
            names.sort();
            plan.conflicts.insert(host.to_owned(), names);
        }
    }
    Ok(plan)
}

/// Detaches the gateway from an instance network so the network can be
/// removed.
///
/// # Errors
///
/// Returns an error when Docker fails for a reason other than the gateway
/// or the network being absent.
pub(crate) fn detach_network(network: &str) -> Result<()> {
    if crate::docker::is_dry_run() {
        return Ok(());
    }
    let gateway = gateway_container_name(&crate::home::installation_id()?);
    let output = crate::docker::run_docker_output_owned(
        &[
            "network".to_owned(),
            "disconnect".to_owned(),
            "--force".to_owned(),
            network.to_owned(),
            gateway,
        ],
        "failed to detach the berth gateway",
    )?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    if output.status.success()
        || stderr.contains("is not connected")
        || stderr.contains("No such")
        || stderr.contains("not found")
    {
        return Ok(());
    }
    anyhow::bail!(
        "failed to detach the berth gateway from {network}: {}",
        stderr.trim()
    )
}

fn discover_routes(installation: &str) -> Result<Vec<LabeledRoute>> {
    let format = format!(
        "{{{{.Names}}}}\t{{{{.Label \"{ROUTE_HOSTS_LABEL}\"}}}}\t{{{{.Label \"{ROUTE_PORT_LABEL}\"}}}}\t{{{{.Label \"{NETWORK_LABEL}\"}}}}"
    );
    let output = crate::docker::run_docker_output_owned(
        &[
            "ps".to_owned(),
            "--filter".to_owned(),
            format!("label=dev.berth.installation={installation}"),
            "--filter".to_owned(),
            format!("label={ROUTE_HOSTS_LABEL}"),
            "--format".to_owned(),
            format,
        ],
        "failed to list routed containers",
    )?;
    if !output.status.success() {
        anyhow::bail!(
            "failed to list routed containers: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(parse_routes(&String::from_utf8_lossy(&output.stdout)))
}

pub(super) fn parse_routes(listing: &str) -> Vec<LabeledRoute> {
    listing
        .lines()
        .filter_map(|line| {
            let mut fields = line.split('\t');
            let container = fields.next()?.trim();
            let hosts = fields.next()?;
            let port = fields.next()?.trim();
            let network = fields.next().unwrap_or_default().trim();
            let hosts: Vec<String> = hosts
                .split(',')
                .map(str::trim)
                .filter(|host| !host.is_empty())
                .map(ToOwned::to_owned)
                .collect();
            (!container.is_empty() && !hosts.is_empty()).then(|| LabeledRoute {
                container: container.to_owned(),
                hosts,
                port: port.to_owned(),
                network: network.to_owned(),
            })
        })
        .collect()
}

/// Attaches the gateway to `network` answering `hosts`, reconnecting when
/// its aliases there differ (Docker cannot change aliases in place). A
/// network removed concurrently by `down` is skipped.
fn attach(
    gateway: &str,
    network: &str,
    hosts: &BTreeSet<String>,
    current: Option<&BTreeSet<String>>,
) -> Result<()> {
    if current == Some(hosts) {
        return Ok(());
    }
    if current.is_some() {
        detach_network(network)?;
    }
    let mut args = vec!["network".to_owned(), "connect".to_owned()];
    for host in hosts {
        args.push("--alias".to_owned());
        args.push(host.clone());
    }
    args.push(network.to_owned());
    args.push(gateway.to_owned());
    let output =
        crate::docker::run_docker_output_owned(&args, "failed to attach the berth gateway")?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    if output.status.success() || stderr.contains("not found") || stderr.contains("No such network")
    {
        return Ok(());
    }
    anyhow::bail!(
        "failed to attach the berth gateway to {network}: {}",
        stderr.trim()
    )
}

/// Writes the document a gateway starts from when none exists: the
/// renderer's output for an empty route set. Routes are then applied by a
/// validated reload.
fn ensure_start_document(dirs: &GatewayDirs, ports: GatewayPorts) -> Result<()> {
    let current = dirs.config.join("caddy.json");
    if current.exists() {
        return Ok(());
    }
    let empty = serde_json::to_string_pretty(&render_config(&[], ports))
        .context("failed to render gateway config")?;
    crate::config::yaml::write_file_atomically(&current, &empty)
}

/// Ensures the gateway runs on `ports` and records them once it does. Must
/// be called under the gateway lock.
///
/// # Errors
///
/// Returns an error when the gateway cannot be started or its ports
/// recorded.
pub(super) fn start_gateway(
    installation: &str,
    ports: GatewayPorts,
    dirs: &GatewayDirs,
) -> Result<()> {
    ensure_start_document(dirs, ports)?;
    if ensure_gateway(installation, ports, dirs)? == GatewayState::Started {
        wait_until_admin_ready(&gateway_container_name(installation))?;
    }
    ports.record()
}

/// Attaches the gateway to `network` for health probes, keeping whatever
/// route aliases it already has there. Must be called under the gateway lock.
///
/// # Errors
///
/// Returns an error when Docker fails.
pub(super) fn attach_for_probes(gateway: &str, network: &str) -> Result<()> {
    if attached_aliases(gateway)?.contains_key(network) {
        return Ok(());
    }
    attach(gateway, network, &BTreeSet::new(), None)
}

/// Whether the gateway exists and is attached to `network`.
///
/// # Errors
///
/// Returns an error when Docker fails or the gateway's networks cannot be
/// parsed.
pub(crate) fn is_attached(network: &str) -> Result<bool> {
    let gateway = gateway_container_name(&crate::home::installation_id()?);
    // A stopped gateway keeps its endpoints, so existence is what matters.
    if !super::container::exists(&gateway)? {
        return Ok(false);
    }
    Ok(attached_aliases(&gateway)?.contains_key(network))
}

/// Removes the gateway container when no instance network has it attached
/// (`berth gc`). Its CA and state under `~/.berth/gateway` stay, so the next
/// `up` recreates it with the same trusted certificate. Returns the removed
/// container, or why it was kept.
///
/// # Errors
///
/// Returns an error when Docker fails.
pub(crate) fn remove_gateway_if_unused() -> Result<std::result::Result<String, String>> {
    let _lock = crate::home::HostLock::acquire("gateway", PUBLISH_LOCK_TIMEOUT)?;
    let gateway = gateway_container_name(&crate::home::installation_id()?);
    if !super::container::exists(&gateway)? {
        return Ok(Err("no gateway exists".to_owned()));
    }
    // Only berth networks hold the gateway; the engine's default network
    // (`bridge` on Docker, `podman` on Podman) is where it starts.
    let in_use: Vec<String> = attached_aliases(&gateway)?
        .into_keys()
        .filter(|network| network.starts_with("berth_"))
        .collect();
    if !in_use.is_empty() {
        return Ok(Err(format!("attached to {}", in_use.join(", "))));
    }

    let output = crate::docker::run_docker_output(
        &["rm", "-f", &gateway],
        "failed to remove the berth gateway",
    )?;
    if !output.status.success() {
        anyhow::bail!(
            "failed to remove the berth gateway: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(Ok(gateway))
}

/// Returns the gateway's berth route aliases on every network it is
/// attached to, from one inspect.
fn attached_aliases(gateway: &str) -> Result<BTreeMap<String, BTreeSet<String>>> {
    let output = crate::docker::run_docker_output_owned(
        &[
            "inspect".to_owned(),
            "--format".to_owned(),
            "{{json .NetworkSettings.Networks}}".to_owned(),
            gateway.to_owned(),
        ],
        "failed to inspect the berth gateway",
    )?;
    if !output.status.success() {
        anyhow::bail!(
            "failed to inspect the berth gateway: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    parse_aliases(&output.stdout)
}

pub(super) fn parse_aliases(json: &[u8]) -> Result<BTreeMap<String, BTreeSet<String>>> {
    let networks: serde_json::Map<String, serde_json::Value> =
        serde_json::from_slice(json).context("failed to parse the berth gateway's networks")?;
    let suffix = format!(".{}", super::route::ROUTE_SUFFIX);
    Ok(networks
        .into_iter()
        .map(|(network, endpoint)| {
            let aliases = endpoint
                .get("Aliases")
                .and_then(serde_json::Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(serde_json::Value::as_str)
                .filter(|alias| alias.ends_with(&suffix))
                .map(ToOwned::to_owned)
                .collect();
            (network, aliases)
        })
        .collect())
}

fn reload(gateway: &str, config: &str) -> Result<()> {
    let output = crate::docker::run_docker_output_owned(
        &[
            "exec".to_owned(),
            gateway.to_owned(),
            "caddy".to_owned(),
            "reload".to_owned(),
            "--config".to_owned(),
            config.to_owned(),
            "--address".to_owned(),
            "localhost:2019".to_owned(),
        ],
        "failed to reload the berth gateway",
    )?;
    if !output.status.success() {
        anyhow::bail!(
            "the berth gateway rejected its new configuration: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(())
}

fn wait_until_admin_ready(gateway: &str) -> Result<()> {
    let started = std::time::Instant::now();
    loop {
        let output = crate::docker::run_docker_output_owned(
            &[
                "exec".to_owned(),
                gateway.to_owned(),
                "wget".to_owned(),
                "-q".to_owned(),
                "-O".to_owned(),
                "/dev/null".to_owned(),
                // busybox resolves `localhost` to `::1` first, where the admin
                // API (bound by Caddy to IPv4 localhost) refuses connections.
                "http://127.0.0.1:2019/config/".to_owned(),
            ],
            "failed to check the berth gateway",
        )?;
        if output.status.success() {
            return Ok(());
        }
        if started.elapsed() >= START_TIMEOUT {
            anyhow::bail!(
                "the berth gateway did not start within {}s; see `docker logs {gateway}`",
                START_TIMEOUT.as_secs()
            );
        }
        std::thread::sleep(Duration::from_millis(250));
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_aliases, parse_routes, plan_routes};

    #[test]
    fn listing_parses_into_routes_and_per_network_aliases() {
        let labeled = parse_routes(
            "berth_p_a_app\ta-p.berth.localhost,shop.berth.localhost\t80\tberth_p_a\n\
             berth_p_b_app\tb-p.berth.localhost\t80\tberth_p_b\n\
             unrouted\t\t\t\n",
        );
        assert_eq!(labeled.len(), 2);
        let plan = plan_routes(&labeled).expect("plan");
        assert_eq!(plan.routes.len(), 3);
        assert!(
            plan.routes
                .iter()
                .any(|route| route.upstream == "berth_p_a_app:80")
        );
        assert_eq!(plan.aliases["berth_p_a"].len(), 2);
        assert_eq!(plan.aliases["berth_p_b"].len(), 1);
        assert!(plan.conflicts.is_empty());
    }

    #[test]
    fn a_host_claimed_twice_is_withheld_from_both_and_others_still_publish() {
        let labeled = parse_routes(
            "one\tx.berth.localhost,one.berth.localhost\t80\tn1\n\
             two\tx.berth.localhost\t80\tn2\n\
             three\tthree.berth.localhost\t80\tn3\n",
        );
        let plan = plan_routes(&labeled).expect("plan");
        let hosts: Vec<&str> = plan
            .routes
            .iter()
            .map(|route| route.host.as_str())
            .collect();
        assert_eq!(hosts, ["one.berth.localhost", "three.berth.localhost"]);
        assert_eq!(plan.conflicts["x.berth.localhost"], ["one", "two"]);
        assert!(!plan.aliases.contains_key("n2"), "n2 has no route left");
    }

    #[test]
    fn an_invalid_port_label_fails_naming_the_container() {
        let labeled = parse_routes("app\tx.berth.localhost\thttp\tn\n");
        assert!(
            plan_routes(&labeled)
                .expect_err("port")
                .to_string()
                .contains("app")
        );
    }

    #[test]
    fn aliases_are_read_per_network_keeping_only_route_names() {
        let aliases = parse_aliases(
            br#"{"berth_p_a":{"Aliases":["a-p.berth.localhost","3f2a"]},"bridge":{"Aliases":null}}"#,
        )
        .expect("aliases");
        assert_eq!(aliases["berth_p_a"].len(), 1);
        assert!(aliases["bridge"].is_empty());
    }
}
