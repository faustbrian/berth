//! config raw module.
//!
//! Contains config raw logic used by Berth command workflows.

use serde::Deserialize;

use super::{ContainerEngine, DomainStrategy, ProjectType};

mod service;
mod service_hook;
mod swarm;
mod swarm_git;

pub(crate) use service::RawServiceConfig;
pub(crate) use service_hook::{RawHookRun, RawServiceHook};
pub(crate) use swarm::RawSwarmTarget;
pub(crate) use swarm_git::RawSwarmGit;

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct RawConfig {
    #[serde(default)]
    pub schema_version: Option<u32>,
    #[serde(default)]
    pub project_type: Option<ProjectType>,
    #[serde(default)]
    pub container_engine: Option<ContainerEngine>,
    pub container_prefix: Option<String>,
    #[serde(default)]
    pub domain_strategy: Option<DomainStrategy>,
    #[serde(default)]
    pub service: Vec<RawServiceConfig>,
    #[serde(default)]
    pub swarm: Vec<RawSwarmTarget>,
}
