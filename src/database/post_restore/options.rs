//! database post restore options module.
//!
//! Contains typed options for Berth command post-restore workflows.

use std::path::Path;

use crate::config::ServiceConfig;

/// Options for Laravel-specific post-restore hooks.
pub(crate) struct PostRestoreOptions<'a> {
    pub(crate) run_migrate: bool,
    pub(crate) run_schema_dump: bool,
    pub(crate) restored_service: &'a ServiceConfig,
    pub(crate) project_root_override: Option<&'a Path>,
    pub(crate) config_path: Option<&'a Path>,
}

impl<'a> PostRestoreOptions<'a> {
    /// Creates a new post-restore options value.
    pub(crate) const fn new(
        run_migrate: bool,
        run_schema_dump: bool,
        restored_service: &'a ServiceConfig,
        project_root_override: Option<&'a Path>,
        config_path: Option<&'a Path>,
    ) -> Self {
        Self {
            run_migrate,
            run_schema_dump,
            restored_service,
            project_root_override,
            config_path,
        }
    }
}
