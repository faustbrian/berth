//! config types swarm module.
//!
//! Contains config types swarm logic used by Berth command workflows.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use super::SwarmGit;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwarmTarget {
    /// Unique swarm target name.
    pub name: String,
    /// Project root for this target.
    pub root: PathBuf,
    /// Other swarm targets that must be active first.
    #[serde(default)]
    pub depends_on: Vec<String>,
    /// Explicit env vars to inject from other workspace targets.
    #[serde(default)]
    pub inject_env: Vec<SwarmInjectEnv>,
    /// Optional git bootstrap configuration for cloning missing roots.
    #[serde(default)]
    pub git: Option<SwarmGit>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SwarmInjectEnv {
    /// Environment variable name to inject.
    pub env: String,
    /// Source workspace target name.
    pub from: String,
    /// Value specifier (`:domain`, `:host`, `:port`, `:scheme`, `:base_url`, `:url`) or literal.
    pub value: String,
    /// Optional app service name within the source project.
    #[serde(default)]
    pub service: Option<String>,
}
