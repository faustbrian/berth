//! Stale testing runtime cleanup helpers.

use anyhow::Result;
use std::collections::HashSet;

use crate::config::ServiceConfig;
use crate::docker::{LABEL_MANAGED, VALUE_MANAGED_TRUE};
use crate::output::{self, LogLevel, Persistence};

pub(super) fn cleanup_stale_testing_runtime_containers(
    startup_services: &[&ServiceConfig],
) -> Result<()> {
    let active_names = startup_services
        .iter()
        .filter_map(|service| service.container_name().ok())
        .collect::<HashSet<_>>();
    let stale_prefixes = active_names
        .iter()
        .filter_map(|container_name| stale_runtime_prefix(container_name))
        .collect::<Vec<_>>();

    if stale_prefixes.is_empty() {
        return Ok(());
    }

    let list_args = stale_cleanup_list_args(&crate::home::installation_id()?);

    if crate::docker::is_dry_run() {
        crate::docker::print_docker_command(&list_args);
        return Ok(());
    }

    let output = crate::docker::run_docker_output_owned(
        &list_args,
        "Failed to list docker containers for stale test runtime cleanup",
    )?;
    let output = crate::docker::ensure_docker_output_success(
        output,
        "Failed to list docker containers for stale test runtime cleanup",
    )?;
    let container_names = String::from_utf8_lossy(&output.stdout);
    let stale_names =
        collect_stale_container_names(&container_names, &active_names, &stale_prefixes);

    for container_name in stale_names {
        let remove_args = vec!["rm".to_owned(), "-f".to_owned(), container_name.clone()];
        let remove_output = crate::docker::run_docker_output_owned(
            &remove_args,
            "Failed to remove stale testing container",
        )?;
        crate::docker::ensure_docker_output_success(
            remove_output,
            "Failed to remove stale testing container",
        )?;
        output::event(
            "artisan",
            LogLevel::Info,
            &format!("Removed stale testing container {container_name}"),
            Persistence::Persistent,
        );
    }

    Ok(())
}

fn stale_cleanup_list_args(installation: &str) -> Vec<String> {
    vec![
        "ps".to_owned(),
        "-a".to_owned(),
        "--filter".to_owned(),
        format!("label={LABEL_MANAGED}={VALUE_MANAGED_TRUE}"),
        // Only this installation's: another one's stopped test containers
        // carry the same names.
        "--filter".to_owned(),
        format!("label=dev.berth.installation={installation}"),
        "--filter".to_owned(),
        "status=exited".to_owned(),
        "--filter".to_owned(),
        "status=dead".to_owned(),
        "--format".to_owned(),
        "{{.Names}}".to_owned(),
    ]
}

fn stale_runtime_prefix(container_name: &str) -> Option<String> {
    let (prefix, run_id) = container_name.rsplit_once('-')?;
    if !prefix.ends_with("-testing") {
        return None;
    }

    if run_id.is_empty() || !run_id.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return None;
    }

    Some(format!("{prefix}-"))
}

fn collect_stale_container_names(
    listed_container_names: &str,
    active_names: &HashSet<String>,
    stale_prefixes: &[String],
) -> Vec<String> {
    listed_container_names
        .lines()
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .filter(|name| stale_prefixes.iter().any(|prefix| name.starts_with(prefix)))
        .filter(|name| !active_names.contains(*name))
        .map(ToOwned::to_owned)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{collect_stale_container_names, stale_cleanup_list_args, stale_runtime_prefix};
    use crate::docker::{LABEL_MANAGED, VALUE_MANAGED_TRUE};
    use std::collections::HashSet;

    #[test]
    fn stale_runtime_prefix_parses_testing_runtime_name() {
        assert_eq!(
            stale_runtime_prefix("shipit-location-db-testing-77634390"),
            Some("shipit-location-db-testing-".to_owned())
        );
    }

    #[test]
    fn stale_runtime_prefix_rejects_non_testing_names() {
        assert_eq!(stale_runtime_prefix("shipit-location-db"), None);
        assert_eq!(stale_runtime_prefix("shipit-location-db-testing"), None);
        assert_eq!(
            stale_runtime_prefix("shipit-location-db-testing-nothex"),
            None
        );
    }

    #[test]
    fn collect_stale_container_names_filters_active_and_unrelated_names() {
        let listed = [
            "shipit-location-db-testing-77634390",
            "shipit-location-db-testing-c9ca3b08",
            "shipit-other-db-testing-c9ca3b08",
        ]
        .join("\n");
        let active = HashSet::from(["shipit-location-db-testing-77634390".to_owned()]);
        let stale_prefixes = vec!["shipit-location-db-testing-".to_owned()];

        assert_eq!(
            collect_stale_container_names(&listed, &active, &stale_prefixes),
            vec!["shipit-location-db-testing-c9ca3b08".to_owned()]
        );
    }

    #[test]
    fn stale_cleanup_list_args_targets_only_stopped_containers() {
        assert_eq!(
            stale_cleanup_list_args("abc123"),
            vec![
                "ps".to_owned(),
                "-a".to_owned(),
                "--filter".to_owned(),
                format!("label={LABEL_MANAGED}={VALUE_MANAGED_TRUE}"),
                "--filter".to_owned(),
                "label=dev.berth.installation=abc123".to_owned(),
                "--filter".to_owned(),
                "status=exited".to_owned(),
                "--filter".to_owned(),
                "status=dead".to_owned(),
                "--format".to_owned(),
                "{{.Names}}".to_owned(),
            ]
        );
    }
}
