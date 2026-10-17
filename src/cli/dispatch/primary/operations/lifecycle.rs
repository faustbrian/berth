//! Lifecycle command dispatch (`up`, `apply`, `update`, `down`, `recreate`, `restart`).

use anyhow::Result;

use crate::cli::args::Cli;
use crate::cli::dispatch::context::CliDispatchContext;
use crate::config::Config;

mod service_ops;
mod up;
mod up_apply;

/// Dispatches lifecycle commands and forwards normalized arguments to handlers.
///
/// `apply` is mapped to a deterministic `up` invocation with fixed defaults.
pub(super) fn dispatch(
    cli: &Cli,
    config: &mut Config,
    context: &CliDispatchContext<'_>,
) -> Option<Result<()>> {
    up_apply::dispatch(cli, config, context).or_else(|| service_ops::dispatch(cli, config, context))
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
        super::dispatch(&cli, &mut config, &context)
    }

    #[test]
    fn lifecycle_prefers_up_apply() {
        assert!(dispatch_result(&["berth", "up"]).is_some());
    }

    #[test]
    fn lifecycle_falls_back_to_service_ops() {
        assert!(dispatch_result(&["berth", "start"]).is_some());
    }

    #[test]
    fn lifecycle_handles_non_matching_commands_as_none() {
        assert!(dispatch_result(&["berth", "status"]).is_none());
    }
}
