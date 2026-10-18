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
    // `--all` keeps another berth installation's containers with a
    // `label!=dev.berth.installation` filter; the engine requires a
    // container to carry every `label!` key to be kept, so another
    // `label!=` would let those containers be pruned. Only the keys
    // `container prune` supports besides it pass, named as Docker reads
    // them (trimmed, case-insensitive).
    for value in filter {
        let key = value
            .split_once('=')
            .map_or(value.as_str(), |(key, _)| key)
            .trim()
            .to_ascii_lowercase();
        if key != "label" && key != "until" {
            anyhow::bail!(
                "--filter {value} is not supported with --all: use `label=` or `until=` filters \
                 (berth keeps other berth installations' containers with a `label!=` filter of \
                 its own)"
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::validate_prune_args;

    #[test]
    fn global_prune_refuses_negated_label_filters() {
        for refused in [
            "label!=foo",
            "LABEL!=foo",
            " Label!=foo",
            "label! =foo",
            "name=x",
        ] {
            let err =
                validate_prune_args(true, true, &["until=24h".to_owned(), refused.to_owned()])
                    .expect_err(refused);
            assert!(err.to_string().contains(refused), "{err}");
        }
        for allowed in ["label=foo", "LABEL=foo", "until=24h", " Until=24h"] {
            assert!(
                validate_prune_args(true, true, &[allowed.to_owned()]).is_ok(),
                "{allowed}"
            );
        }
    }

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
