//! Dispatch for operational commands (lifecycle, management, access, setup).

use anyhow::Result;

use crate::cli::args::Cli;
use crate::cli::dispatch::context::CliDispatchContext;
use crate::config::Config;

mod access;
mod lifecycle;
mod management;
mod setup;

/// Dispatches operational commands in precedence order.
///
/// `lifecycle`, then `management`, then `access`, then fallback `setup`.
pub(super) fn dispatch_operation_commands(
    cli: &Cli,
    config: &mut Config,
    context: &CliDispatchContext<'_>,
) -> Option<Result<()>> {
    lifecycle::dispatch(cli, config, context)
        .or_else(|| management::dispatch(cli, config, context))
        .or_else(|| access::dispatch(cli, config, context))
        .or_else(|| setup::dispatch(cli, config, context))
}

#[cfg(test)]
mod tests {
    use crate::cli::{args::Cli, dispatch::context::CliDispatchContext};
    use crate::config::Config;
    use clap::Parser;

    fn sample_config() -> Config {
        Config {
            schema_version: 1,
            project_type: crate::config::ProjectType::Project,
            container_prefix: None,
            domain_strategy: None,
            service: Vec::new(),
            swarm: Vec::new(),
        }
    }

    fn dispatch_result(args: &[&str]) -> Option<Result<(), anyhow::Error>> {
        let cli = Cli::parse_from(args);
        let context = CliDispatchContext::from_cli(&cli);
        let mut config = sample_config();
        super::dispatch_operation_commands(&cli, &mut config, &context)
    }

    #[test]
    fn operation_dispatch_prioritizes_lifecycle() {
        assert!(dispatch_result(&["berth", "up"]).is_some());
    }

    #[test]
    fn operation_dispatch_handles_management_when_lifecycle_does_not_match() {
        assert!(dispatch_result(&["berth", "stop"]).is_some());
    }

    #[test]
    fn operation_dispatch_routes_to_access_command() {
        assert!(dispatch_result(&["berth", "url"]).is_some());
    }

    #[test]
    fn operation_dispatch_returns_none_for_primary_meta() {
        assert!(dispatch_result(&["berth", "about"]).is_none());
    }
}
