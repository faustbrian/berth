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
    /// The requested `isolation`, if any.
    pub isolation: Option<super::Isolation>,
    /// Whether `.berth.yaml` sets `database`, `username` or `password` for
    /// the service (rather than its preset).
    pub authored_credentials: bool,
    /// The shared server and tenant backing the service, if it is shared.
    pub shared: Option<SharedTarget>,
}

/// A service backed by a tenant in a shared server.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SharedTarget {
    /// Engine of the shared server.
    pub engine: SharedEngine,
    /// Container name of the shared server.
    pub server: String,
    /// Image the shared server runs.
    pub image: String,
    /// Compatibility fingerprint (hex) the server is keyed by.
    pub fingerprint: String,
}

/// Engines berth shares between instances.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SharedEngine {
    /// PostgreSQL: a database and login role per tenant.
    Postgres,
}

impl SharedEngine {
    /// The engine's name in container names and labels.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Postgres => "postgres",
        }
    }
}
