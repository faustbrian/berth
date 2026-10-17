//! cli support for persisting runtime port changes.
//!
//! Centralizes env updates that occur after services are started with
//! randomized or reassigned ports. Berth never writes runtime ports back into
//! the committed project file.

use anyhow::Result;
use std::path::Path;

use crate::{config, env};

/// Writes runtime port changes into the selected env file, when one is set.
pub(crate) fn persist_random_runtime_ports<'a, I>(
    services: I,
    env_path: Option<&Path>,
) -> Result<()>
where
    I: IntoIterator<Item = &'a config::ServiceConfig>,
{
    let Some(path) = env_path else {
        return Ok(());
    };
    for service in services {
        env::update_env(service, path, true)?;
    }
    Ok(())
}
