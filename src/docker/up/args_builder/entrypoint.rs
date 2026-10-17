//! docker up args builder entrypoint module.
//!
//! Contains docker up args builder entrypoint logic used by Berth command workflows.

use crate::config::{Driver, ServiceConfig};

/// Appends entrypoint args to the caller-provided command or collection.
pub(super) fn append_entrypoint_args(args: &mut Vec<String>, service: &ServiceConfig) {
    if matches!(service.driver, Driver::Minio) {
        args.push("server".to_owned());
        args.push("/data".to_owned());

        if service
            .env
            .as_ref()
            .is_none_or(|env| !env.contains_key("MINIO_CONSOLE_ADDRESS"))
        {
            args.push("--console-address".to_owned());
            args.push(":9001".to_owned());
        }
    }

    if let Some(command) = &service.command {
        args.extend(command.iter().cloned());
    }
}
