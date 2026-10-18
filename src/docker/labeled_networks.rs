//! Networks matching label filters, with chosen labels, on every engine.

use anyhow::Result;

/// Lists the networks matching `filters` (`--filter` values) as their name
/// followed by the values of `keys`.
///
/// `network ls` cannot print one label on both engines (Podman 4.9 renders
/// its labels as a list), so names come from `network ls -q` and labels
/// from one `network inspect` of them all, whose `.Labels` is a map on both.
/// A network removed between the two calls is skipped.
///
/// # Errors
///
/// Returns an error when the engine cannot list or inspect networks.
pub(crate) fn labeled_networks(filters: &[String], keys: &[&str]) -> Result<Vec<Vec<String>>> {
    let mut list = vec!["network".to_owned(), "ls".to_owned(), "-q".to_owned()];
    for filter in filters {
        list.extend(["--filter".to_owned(), filter.clone()]);
    }
    let listed = super::run_docker_output_owned(&list, "failed to list networks")?;
    if !listed.status.success() {
        anyhow::bail!(
            "failed to list networks: {}",
            String::from_utf8_lossy(&listed.stderr).trim()
        );
    }
    let ids: Vec<String> = String::from_utf8_lossy(&listed.stdout)
        .lines()
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .map(ToOwned::to_owned)
        .collect();
    let mut format = "{{.Name}}".to_owned();
    for key in keys {
        format.push_str(&format!("\t{{{{index .Labels \"{key}\"}}}}"));
    }
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let mut batch = vec![
        "network".to_owned(),
        "inspect".to_owned(),
        "--format".to_owned(),
        format.clone(),
    ];
    batch.extend(ids.iter().cloned());
    let inspected = super::run_docker_output_owned(&batch, "failed to inspect networks")?;
    if inspected.status.success() {
        return Ok(rows(&inspected.stdout));
    }
    // One of them went away in between: inspect each, skipping the gone.
    let mut networks = Vec::new();
    for id in ids {
        let inspected = super::run_docker_output(
            &["network", "inspect", "--format", &format, &id],
            "failed to inspect a network",
        )?;
        if !inspected.status.success() {
            let stderr = String::from_utf8_lossy(&inspected.stderr);
            let lowered = stderr.to_ascii_lowercase();
            if lowered.contains("no such network") || lowered.contains("not found") {
                continue;
            }
            anyhow::bail!("failed to inspect network {id}: {}", stderr.trim());
        }
        networks.extend(rows(&inspected.stdout));
    }
    Ok(networks)
}

fn rows(stdout: &[u8]) -> Vec<Vec<String>> {
    String::from_utf8_lossy(stdout)
        .lines()
        .map(|line| line.split('\t').map(ToOwned::to_owned).collect())
        .collect()
}
