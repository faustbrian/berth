//! cli handlers docker ops events module.
//!
//! Contains events handler used by Berth command workflows.

use anyhow::Result;

use crate::docker::{LABEL_CONTAINER, LABEL_MANAGED, VALUE_MANAGED_TRUE};
use crate::{config, docker};

pub(crate) struct HandleEventsOptions<'a> {
    pub(crate) service: Option<&'a str>,
    pub(crate) kind: Option<config::Kind>,
    pub(crate) since: Option<&'a str>,
    pub(crate) until: Option<&'a str>,
    pub(crate) format: Option<&'a str>,
    pub(crate) json: bool,
    pub(crate) all: bool,
    pub(crate) allow_empty: bool,
    pub(crate) filter: &'a [String],
}

pub(crate) fn handle_events(
    config: &config::Config,
    options: HandleEventsOptions<'_>,
) -> Result<()> {
    let effective_format = if options.json {
        Some("{{json .}}")
    } else {
        options.format
    };

    if options.all {
        super::log::warn(&format!(
            "Streaming global {} events via --all",
            docker::runtime_event_source_label()
        ));
        return docker::events(
            options.since,
            options.until,
            effective_format,
            options.filter,
        );
    }

    let selected = super::selected_docker_services(config, options.service, options.kind)?;
    if selected.is_empty() {
        if !options.allow_empty {
            anyhow::bail!("No services matched current filters; use --allow-empty to ignore");
        }
        super::log::info("No services matched filters; no events to stream");
        return Ok(());
    }

    let container_names = selected
        .into_iter()
        .map(|svc| svc.container_name())
        .collect::<Result<Vec<String>>>()?;
    let all_filters = build_scoped_event_filters(options.filter, &container_names);
    super::log::info(&format!(
        "Streaming Berth-scoped {} events",
        docker::runtime_event_source_label()
    ));
    docker::events(options.since, options.until, effective_format, &all_filters)
}

fn build_scoped_event_filters(base_filters: &[String], container_names: &[String]) -> Vec<String> {
    let mut all_filters = Vec::with_capacity(base_filters.len() + container_names.len() + 1);
    all_filters.extend(base_filters.iter().cloned());
    all_filters.push(format!("label={LABEL_MANAGED}={VALUE_MANAGED_TRUE}"));
    all_filters.extend(
        container_names
            .iter()
            .map(|container_name| format!("label={LABEL_CONTAINER}={container_name}")),
    );
    all_filters
}

#[cfg(test)]
mod tests {
    use super::build_scoped_event_filters;

    #[test]
    fn builds_label_scoped_event_filters() {
        let base = vec!["type=container".to_owned()];
        let containers = vec!["acme-db".to_owned(), "acme-app".to_owned()];
        let filters = build_scoped_event_filters(&base, &containers);
        assert!(filters.contains(&"type=container".to_owned()));
        assert!(filters.contains(&"label=dev.berth.managed=true".to_owned()));
        assert!(filters.contains(&"label=dev.berth.container=acme-db".to_owned()));
        assert!(filters.contains(&"label=dev.berth.container=acme-app".to_owned()));
    }
}
