//! docker up args builder entrypoint module.
//!
//! Contains docker up args builder entrypoint logic used by Berth command workflows.

use crate::config::ServiceConfig;

/// Appends entrypoint args to the caller-provided command or collection.
pub(super) fn append_entrypoint_args(args: &mut Vec<String>, service: &ServiceConfig) {
    if let Some(command) = &service.command {
        args.extend(command.iter().cloned());
    }
}
