//! A container's CPU and memory caps as engine `run` options.

use crate::config::ServiceConfig;

/// `--cpus` and `--memory` from the service's `cpus` and `memory`, with
/// `--memory-swap` equal to `--memory` so the cap holds without swap
/// (Docker's and Podman's default allows as much swap again).
pub(crate) fn resource_limit_args(service: &ServiceConfig) -> Vec<String> {
    let limits = &service.runtime.limits;
    let mut args = Vec::new();
    if let Some(cpus) = &limits.cpus {
        args.extend(["--cpus".to_owned(), cpus.clone()]);
    }
    if let Some(memory) = &limits.memory {
        args.extend([
            "--memory".to_owned(),
            memory.clone(),
            "--memory-swap".to_owned(),
            memory.clone(),
        ]);
    }
    args
}
