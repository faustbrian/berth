//! Per-invocation runtime identity attached to a resolved service.

use std::path::PathBuf;

/// Where a service runs: its project, instance and canonical project root.
///
/// Filled in when a project file is loaded; empty for services built in
/// memory (presets previews and tests). These values become the service's
/// ownership labels.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
#[non_exhaustive]
pub struct ServiceRuntime {
    /// Project name from `.berth.yaml`.
    pub project: Option<String>,
    /// Instance ID (see `crate::instance`).
    pub instance: Option<String>,
    /// Canonical project root of the instance.
    pub root: Option<PathBuf>,
    /// Docker network of the instance (and runtime environment).
    pub network: Option<String>,
}
