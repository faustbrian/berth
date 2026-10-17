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
    /// HTTPS port of the gateway serving this service's routes; set only for
    /// services that have gateway routes.
    pub gateway_https_port: Option<u16>,
    /// Whether the service's port is published on the host (`publish: true`,
    /// `--publish-all`, runtime environments and test leases).
    pub publish_host_port: bool,
}
