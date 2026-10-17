//! config raw swarm module.
//!
//! Contains config raw swarm logic used by Berth command workflows.

use serde::Deserialize;
use std::path::PathBuf;

use super::super::types::SwarmInjectEnv;
use super::RawSwarmGit;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RawSwarmTarget {
    pub name: String,
    pub root: PathBuf,
    #[serde(default)]
    pub depends_on: Vec<String>,
    #[serde(default)]
    pub inject_env: Vec<SwarmInjectEnv>,
    #[serde(default)]
    pub git: Vec<RawSwarmGit>,
}
