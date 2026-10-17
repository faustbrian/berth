//! cli handlers docker ops prune module.
//!
//! Contains prune handler used by Berth command workflows.

use anyhow::Result;

use crate::{config, docker};

pub(crate) struct HandlePruneOptions<'a> {
    pub(crate) service: Option<&'a str>,
    pub(crate) kind: Option<config::Kind>,
    pub(crate) parallel: usize,
    pub(crate) all: bool,
    pub(crate) force: bool,
    pub(crate) filter: &'a [String],
}

pub(crate) fn handle_prune(config: &config::Config, options: HandlePruneOptions<'_>) -> Result<()> {
    validate_prune_args(options.all, options.force, options.filter)?;

    if options.all {
        super::log::warn(&format!(
            "Pruning all stopped {} containers via --all",
            docker::runtime_log_source_key()
        ));
        return docker::prune(docker::PruneOptions {
            force: options.force,
            filters: options.filter,
        });
    }

    super::run_for_each_docker_service(
        config,
        options.service,
        options.kind,
        options.parallel,
        docker::prune_stopped_container,
    )
}

fn validate_prune_args(all: bool, force: bool, filter: &[String]) -> Result<()> {
    if all && !force {
        anyhow::bail!("--all requires --force to avoid accidental global prune");
    }
    if force && !all {
        anyhow::bail!("--force can only be used with --all");
    }
    if !filter.is_empty() && !all {
        anyhow::bail!("--filter can only be used with --all");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::validate_prune_args;

    #[test]
    fn global_prune_requires_force() {
        let err = validate_prune_args(true, false, &[]).expect_err("expected validation failure");
        assert!(err.to_string().contains("--all requires --force"));
    }

    #[test]
    fn local_prune_rejects_force_and_filter() {
        assert!(validate_prune_args(false, true, &[]).is_err());
        assert!(validate_prune_args(false, false, &["label=x".to_owned()]).is_err());
    }
}
