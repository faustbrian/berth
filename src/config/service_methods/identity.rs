//! config service methods identity module.
//!
//! Contains config service methods identity logic used by Berth command workflows.

use anyhow::{Result, anyhow};

use super::ServiceConfig;
use crate::config::RestartPolicy;

impl ServiceConfig {
    /// Returns the Docker container name for this service.
    #[must_use]
    pub fn container_name(&self) -> Result<String> {
        self.resolved_container_name
            .clone()
            .or_else(|| self.container_name.clone())
            .ok_or_else(|| anyhow!("service '{}' has no resolved container name", self.name))
    }

    #[must_use]
    pub fn scheme(&self) -> &str {
        self.scheme.as_deref().unwrap_or("http")
    }

    #[must_use]
    pub fn resolved_restart_policy(&self) -> RestartPolicy {
        self.restart.unwrap_or(RestartPolicy::UnlessStopped)
    }
}
