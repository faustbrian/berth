//! docker logs args module.
//!
//! Contains docker logs args logic used by Berth command workflows.

use super::LogsOptions;

/// Builds logs args for command execution.
pub(super) fn build_logs_args(container_name: &str, options: LogsOptions) -> Vec<String> {
    let mut args = vec!["logs".to_owned()];
    if options.follow {
        args.push("-f".to_owned());
    }
    if options.timestamps {
        args.push("--timestamps".to_owned());
    }
    if let Some(n) = options.tail {
        args.push("--tail".to_owned());
        args.push(n.to_string());
    }
    if let Some(since) = options.since {
        args.push("--since".to_owned());
        args.push(since);
    }
    if let Some(until) = options.until {
        args.push("--until".to_owned());
        args.push(until);
    }
    args.push(container_name.to_owned());
    args
}
