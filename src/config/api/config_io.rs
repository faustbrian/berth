//! Shared project file and lockfile I/O helpers for config API modules.

use anyhow::{Context, Result};
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::path::{Path, PathBuf};

use super::super::RawConfig;
use super::super::project_file;

pub(super) fn resolve_config_path(
    options: super::project::ProjectRootPathOptions<'_>,
) -> Result<PathBuf> {
    super::super::paths::resolve_config_path(
        options.config_path,
        options.project_root,
        options.runtime_env,
    )
}

pub(super) fn resolve_lockfile_path(
    options: super::project::ProjectRootPathOptions<'_>,
) -> Result<PathBuf> {
    super::super::paths::resolve_lockfile_path(options.config_path, options.project_root)
}

/// Reads and strictly parses one `.berth.yaml` project file.
pub(super) fn read_project_file(path: &Path) -> Result<RawConfig> {
    let name = path
        .file_name()
        .and_then(std::ffi::OsStr::to_str)
        .unwrap_or_default();
    if project_file::LEGACY_CONFIG_FILE_NAMES.contains(&name) || name.ends_with(".toml") {
        return Err(project_file::legacy_config_error(
            path,
            "a .berth.yaml project file",
        ));
    }
    if !name.ends_with(".yaml") {
        anyhow::bail!(
            "berth project files must be YAML (.berth.yaml); got {}",
            path.display()
        );
    }

    let content = crate::config::yaml::read_bounded_file(path)?;
    project_file::parse_project_file(&content, path)
}

pub(super) fn read_yaml_file<T>(path: &Path, label: &str) -> Result<T>
where
    T: DeserializeOwned,
{
    let content = crate::config::yaml::read_bounded_file(path)
        .with_context(|| format!("failed to read {label}"))?;
    crate::config::yaml::from_str(&content)
        .with_context(|| format!("failed to parse {label} at {}", path.display()))
}

pub(super) fn write_yaml_file<T>(path: &Path, value: &T, label: &str) -> Result<()>
where
    T: Serialize,
{
    let content = crate::config::yaml::to_string(value)
        .with_context(|| format!("failed to serialize {label}"))?;
    crate::config::yaml::write_file_atomically(path, &content)
        .with_context(|| format!("failed to write {label} at {}", path.display()))
}
