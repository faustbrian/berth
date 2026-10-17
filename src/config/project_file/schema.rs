//! Strict `.berth.yaml` schema and its conversion into the raw config model.

use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::Path;

use super::PROJECT_SCHEMA_VERSION;
use crate::config::{
    ContainerEngine, DomainStrategy, ProjectType, RawConfig, RawServiceConfig, RawSwarmTarget,
};

/// Maximum project name length, so `{instance}-{project}-{service}` routes
/// stay within one 63-byte DNS label.
pub(crate) const MAX_PROJECT_NAME_LEN: usize = 24;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProjectFile {
    schema_version: u32,
    project: String,
    #[serde(default)]
    project_type: Option<ProjectType>,
    #[serde(default)]
    container_engine: Option<ContainerEngine>,
    #[serde(
        default,
        deserialize_with = "crate::config::unique_map::unique_ordered_map"
    )]
    services: Vec<(String, RawServiceConfig)>,
    #[serde(default)]
    swarm: Vec<RawSwarmTarget>,
}

/// Parses `.berth.yaml` content into the raw config model.
///
/// # Errors
///
/// Returns an error for malformed YAML, unknown fields, an unsupported schema
/// version, an invalid project name, or a service entry that sets `name`.
pub(crate) fn parse_project_file(content: &str, path: &Path) -> Result<RawConfig> {
    let file: ProjectFile = crate::config::yaml::from_str(content)
        .with_context(|| format!("invalid berth project file {}", path.display()))?;

    if file.schema_version != PROJECT_SCHEMA_VERSION {
        anyhow::bail!(
            "{} declares schema_version {}; this berth build reads schema_version {}",
            path.display(),
            file.schema_version,
            PROJECT_SCHEMA_VERSION
        );
    }
    validate_project_name(&file.project)
        .with_context(|| format!("invalid `project` in {}", path.display()))?;

    let mut services = Vec::with_capacity(file.services.len());
    for (name, mut service) in file.services {
        if service.name.is_some() {
            anyhow::bail!(
                "service '{name}' in {} sets `name`; service names come from their `services` key",
                path.display()
            );
        }
        service.name = Some(name);
        services.push(service);
    }

    Ok(RawConfig {
        schema_version: Some(1),
        project_type: file.project_type,
        container_engine: file.container_engine,
        container_prefix: Some(file.project),
        // Until gateway routes replace generated domains, v9 projects use
        // the 7.0 `directory` domain strategy.
        domain_strategy: Some(DomainStrategy::Directory),
        service: services,
        swarm: file.swarm,
    })
}

/// Validates a project name as a lowercase DNS label of bounded length.
///
/// # Errors
///
/// Returns an error naming the exact rejected input.
pub(crate) fn validate_project_name(name: &str) -> Result<()> {
    let bytes = name.as_bytes();
    let valid_chars = bytes
        .iter()
        .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'-');
    let valid_edges = bytes.first().is_some_and(u8::is_ascii_alphanumeric)
        && bytes.last().is_some_and(u8::is_ascii_alphanumeric);
    if !valid_chars || !valid_edges {
        anyhow::bail!("project name '{name}' must be lowercase letters, digits and inner hyphens");
    }
    if name.len() > MAX_PROJECT_NAME_LEN {
        anyhow::bail!(
            "project name '{name}' is {} bytes; the maximum is {MAX_PROJECT_NAME_LEN}",
            name.len()
        );
    }
    Ok(())
}
