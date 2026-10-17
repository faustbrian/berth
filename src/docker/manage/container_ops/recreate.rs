//! docker manage container ops recreate module.
//!
//! Contains docker manage container ops recreate logic used by Berth command workflows.

use anyhow::Result;

use crate::config::ServiceConfig;
use crate::output::{self, LogLevel, Persistence};

use super::super::super::{PullPolicy, is_dry_run, print_docker_command};
use super::docker_cmd::try_docker_output;

pub(super) fn recreate(service: &ServiceConfig) -> Result<()> {
    let container_name = service.container_name()?;

    if is_dry_run() {
        print_docker_command(&["stop".to_owned(), container_name.clone()]);
        print_docker_command(&["rm".to_owned(), "-v".to_owned(), container_name]);
        output::event(
            &service.name,
            LogLevel::Info,
            &format!("[dry-run] Purge and recreate container {}", service.name),
            Persistence::Transient,
        );
        return Ok(());
    }

    try_docker_output(&["stop", &container_name]);
    try_docker_output(&["rm", "-v", &container_name]);

    output::event(
        &service.name,
        LogLevel::Success,
        &format!("Purged container {container_name}"),
        Persistence::Persistent,
    );

    super::super::super::up(service, PullPolicy::Missing, false)
}
