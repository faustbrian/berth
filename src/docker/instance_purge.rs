//! Deletes everything an instance owns in Docker, across all of its runtime
//! environments (`--env` namespaces and test leases).

use anyhow::Result;

/// Removes every container labeled as this installation's instance (with its
/// anonymous volumes), the instance's networks, and the named volumes in the
/// instance namespace `berth_{project}_{instance}`.
///
/// # Errors
///
/// Returns the first Docker failure after attempting every removal.
pub(crate) fn purge_instance_objects(project: &str, instance: &str) -> Result<()> {
    if crate::docker::is_dry_run() {
        crate::output::event(
            "down",
            crate::output::LogLevel::Info,
            &format!("[dry-run] Remove every container, network and volume of {instance}"),
            crate::output::Persistence::Transient,
        );
        return Ok(());
    }
    let installation = crate::home::installation_id()?;
    let filters = [
        format!("label=dev.berth.installation={installation}"),
        format!("label=dev.berth.project={project}"),
        format!("label=dev.berth.instance={instance}"),
    ];
    let mut failures = Vec::new();

    let mut ps = vec![
        "ps".to_owned(),
        "-a".to_owned(),
        "--format".to_owned(),
        "{{.Names}}".to_owned(),
    ];
    for filter in &filters {
        ps.extend(["--filter".to_owned(), filter.clone()]);
    }
    for container in lines(&ps)? {
        record(
            &mut failures,
            &["rm", "-f", "-v", &container],
            &["no such container"],
        );
    }

    let mut networks = vec![
        "network".to_owned(),
        "ls".to_owned(),
        "--format".to_owned(),
        "{{.Name}}".to_owned(),
    ];
    for filter in &filters {
        networks.extend(["--filter".to_owned(), filter.clone()]);
    }
    for network in lines(&networks)? {
        if let Err(error) = crate::gateway::detach_network(&network) {
            failures.push(format!("{error:#}"));
        }
        // The instance's own containers are gone; what is still attached is
        // berth's: shared servers (on every runtime environment's network
        // that used them) and other projects' app containers that joined as
        // swarm consumers (L205). Containers berth did not create are left
        // alone, and then the network stays.
        let attached = [
            "ps".to_owned(),
            "-a".to_owned(),
            "--format".to_owned(),
            "{{.Names}}".to_owned(),
            "--filter".to_owned(),
            format!("network={network}"),
            "--filter".to_owned(),
            "label=dev.berth.managed=true".to_owned(),
            "--filter".to_owned(),
            format!("label=dev.berth.installation={installation}"),
        ];
        for server in lines(&attached)? {
            record(
                &mut failures,
                &["network", "disconnect", "--force", &network, &server],
                &[
                    "is not connected",
                    "not found",
                    "no such container",
                    "no such network",
                ],
            );
        }
        record(
            &mut failures,
            &["network", "rm", &network],
            &["not found", "no such network"],
        );
    }

    // `_` ends the instance ID, which cannot contain one, so `agent-1` never
    // matches `agent-1-b`. Every volume berth creates for an instance (default
    // `{container}-data`, authored, env-suffixed) starts with this prefix.
    // A volume labeled by another berth installation (another `BERTH_HOME`
    // using the same project and instance names) is never this one's.
    let namespace = format!("berth_{project}_{instance}_");
    let installation = crate::home::installation_id()?;
    let volumes = [
        "volume".to_owned(),
        "ls".to_owned(),
        "--format".to_owned(),
        format!(
            "{{{{.Name}}}}\t{}",
            super::label_template("dev.berth.installation")
        ),
    ];
    for line in lines(&volumes)? {
        let (volume, owner) = line.split_once('\t').unwrap_or((line.as_str(), ""));
        let foreign = !owner.is_empty() && owner != installation;
        if volume.starts_with(&namespace) && !foreign {
            record(
                &mut failures,
                &["volume", "rm", volume],
                &["no such volume"],
            );
        }
    }

    match failures.first() {
        None => Ok(()),
        Some(first) => anyhow::bail!("failed to purge instance {instance}: {first}"),
    }
}

fn lines(args: &[String]) -> Result<Vec<String>> {
    let output = crate::docker::run_docker_output_owned(args, "failed to list instance objects")?;
    if !output.status.success() {
        anyhow::bail!(
            "failed to list instance objects: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToOwned::to_owned)
        .collect())
}

fn record(failures: &mut Vec<String>, args: &[&str], benign: &[&str]) {
    match crate::docker::run_docker_output(args, "failed to remove an instance object") {
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr).to_ascii_lowercase();
            if !output.status.success() && !benign.iter().any(|text| stderr.contains(text)) {
                failures.push(format!("docker {}: {}", args.join(" "), stderr.trim()));
            }
        }
        Err(error) => failures.push(format!("{error:#}")),
    }
}
