//! config api migrate module.
//!
//! Converts helm, stackctl and pre-9 berth project files in one directory into
//! berth 9 `.berth.yaml` files.

use anyhow::{Context, Result};
use std::path::PathBuf;

use super::super::project_file::{self, MigratedProjectFile};
use super::project::ProjectRootPathOptions;

pub type MigrateConfigOptions<'a> = ProjectRootPathOptions<'a>;

/// Converts the legacy project files of the selected directory.
///
/// The directory is the parent of `--config` (which must name a legacy project
/// file), else `--project-root`, else the current directory. Discovery does not climb parents, because a parent's
/// legacy file belongs to a different project.
///
/// # Errors
///
/// Returns an error when the directory has no legacy file, a berth 9 file
/// already exists, or a conversion fails.
pub(crate) fn migrate_config_with(
    options: MigrateConfigOptions<'_>,
) -> Result<Vec<MigratedProjectFile>> {
    let dir: PathBuf = if let Some(path) = options.config_path {
        let name = path
            .file_name()
            .and_then(std::ffi::OsStr::to_str)
            .unwrap_or_default();
        if !project_file::is_migratable_file_name(name) {
            anyhow::bail!(
                "--config {} is not a helm or stackctl project file name; `config migrate` converts \
                 .helm.*, .stackctl.* and .berth.toml files in that directory",
                path.display()
            );
        }
        match path.parent() {
            Some(parent) if !parent.as_os_str().is_empty() => parent.to_path_buf(),
            _ => PathBuf::from("."),
        }
    } else if let Some(root) = options.project_root {
        root.to_path_buf()
    } else {
        std::env::current_dir().context("failed to get current directory")?
    };
    project_file::migrate_project_file(&dir)
}
