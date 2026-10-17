//! Shared random-port runtime execution and persistence flow.

use anyhow::Result;
use std::path::Path;

use crate::config;

use super::{persist_random_runtime_ports, run_services_with_app_last};

pub(crate) struct RandomPortsPersistenceOptions<'a> {
    pub(crate) env_path: Option<&'a Path>,
}

pub(crate) fn run_random_ports_flow<'a, T, FIsApp, FRun, FService>(
    planned: &'a [T],
    parallel: usize,
    is_app: FIsApp,
    runtime_runner: FRun,
    service_from_item: FService,
    persistence: RandomPortsPersistenceOptions<'_>,
) -> Result<()>
where
    T: Sync,
    FIsApp: Fn(&T) -> bool + Sync,
    FRun: Fn(&T) -> Result<()> + Sync + Send,
    FService: Fn(&'a T) -> &'a config::ServiceConfig,
{
    run_services_with_app_last(parallel, planned, is_app, runtime_runner)?;

    persist_random_runtime_ports(planned.iter().map(service_from_item), persistence.env_path)
}
