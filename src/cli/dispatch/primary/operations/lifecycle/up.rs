//! Argument translation for `up`-style lifecycle commands.
//!
//! This module converts CLI enums/flags into the handler-level types consumed by
//! `handle_up`.

use anyhow::Result;

use crate::cli::args::PortStrategyArg;
use crate::cli::args::PullPolicyArg;
use crate::cli::dispatch::context::CliDispatchContext;
use crate::cli::handlers;
use crate::config::{Config, Kind};
use crate::docker;

pub(super) struct DispatchUpOptions<'a> {
    pub(super) service: Option<&'a str>,
    pub(super) kind: Option<Kind>,
    pub(super) profile: Option<&'a str>,
    pub(super) wait: bool,
    pub(super) no_wait: bool,
    pub(super) wait_timeout: u64,
    pub(super) pull: PullPolicyArg,
    pub(super) force_recreate: bool,
    pub(super) publish_all: bool,
    pub(super) no_publish_all: bool,
    pub(super) port_strategy: PortStrategyArg,
    pub(super) port_seed: Option<&'a str>,
    pub(super) env_output: bool,
    pub(super) include_project_deps: bool,
    pub(super) seed: bool,
    pub(super) parallel: usize,
}

pub(super) fn dispatch_up(
    config: &mut Config,
    context: &CliDispatchContext<'_>,
    options: DispatchUpOptions<'_>,
) -> Result<()> {
    handlers::handle_up(
        config,
        handlers::HandleUpOptions {
            service: options.service,
            kind: options.kind,
            profile: options.profile,
            wait: options.wait,
            no_wait: options.no_wait,
            wait_timeout: options.wait_timeout,
            pull_policy: pull_policy_from_arg(options.pull),
            force_recreate: options.force_recreate,
            publish_all: options.publish_all,
            no_publish_all: options.no_publish_all,
            port_strategy: options.port_strategy,
            port_seed: options.port_seed,
            env_output: options.env_output,
            include_project_deps: options.include_project_deps,
            seed: options.seed,
            parallel: options.parallel,
            quiet: context.quiet(),
            no_color: context.no_color(),
            dry_run: context.dry_run(),
            repro: context.repro(),
            runtime_env: context.runtime_env(),
            config_path: context.config_path(),
            project_root: context.project_root(),
        },
    )
}

/// Converts the CLI pull policy enum to the docker execution enum.
pub(super) fn pull_policy_from_arg(arg: PullPolicyArg) -> docker::PullPolicy {
    match arg {
        PullPolicyArg::Always => docker::PullPolicy::Always,
        PullPolicyArg::Missing => docker::PullPolicy::Missing,
        PullPolicyArg::Never => docker::PullPolicy::Never,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::args::PullPolicyArg;

    #[test]
    fn pull_policy_from_arg_supports_all_variants() {
        assert!(matches!(
            pull_policy_from_arg(PullPolicyArg::Always),
            docker::PullPolicy::Always
        ));
        assert!(matches!(
            pull_policy_from_arg(PullPolicyArg::Missing),
            docker::PullPolicy::Missing
        ));
        assert!(matches!(
            pull_policy_from_arg(PullPolicyArg::Never),
            docker::PullPolicy::Never
        ));
    }
}
