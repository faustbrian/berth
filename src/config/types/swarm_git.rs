//! config types swarm git module.
//!
//! Contains config types swarm git logic used by Berth command workflows.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwarmGit {
    /// Git repository URL to clone when root is missing.
    pub repo: String,
    /// Optional branch to clone. Uses remote default branch when unset.
    #[serde(default)]
    pub branch: Option<String>,
}
