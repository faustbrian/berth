//! config raw swarm git module.
//!
//! Contains config raw swarm git logic used by Berth command workflows.

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RawSwarmGit {
    pub repo: String,
    #[serde(default)]
    pub branch: Option<String>,
}
