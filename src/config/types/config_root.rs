//! config types config root module.
//!
//! Contains config types config root logic used by Berth command workflows.

use serde::{Deserialize, Serialize};

use super::{DomainStrategy, ProjectType, ServiceConfig, SwarmTarget};

/// Root configuration structure.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Config {
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    /// Project archetype.
    #[serde(default = "default_project_type")]
    pub project_type: ProjectType,
    /// Global container name prefix used when service-level name is not set.
    pub container_prefix: Option<String>,
    /// Project-wide strategy for generating default app domains.
    #[serde(default)]
    pub domain_strategy: Option<DomainStrategy>,
    /// List of service configurations.
    #[serde(default)]
    pub service: Vec<ServiceConfig>,
    /// Optional swarm workspace targets.
    #[serde(default)]
    pub swarm: Vec<SwarmTarget>,
}

/// Returns the default value for schema version.
const fn default_schema_version() -> u32 {
    1
}

const fn default_project_type() -> ProjectType {
    ProjectType::Project
}
