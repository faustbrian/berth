//! cli handlers logs cmd module.
//!
//! Contains cli handlers logs cmd logic used by Berth command workflows.

use anyhow::Result;

use crate::{cli, config, docker};

pub(crate) struct HandleLogsOptions<'a> {
    pub(crate) service: Option<&'a str>,
    pub(crate) services: &'a [String],
    pub(crate) kind: Option<config::Kind>,
    pub(crate) profile: Option<&'a str>,
    pub(crate) all: bool,
    pub(crate) follow: bool,
    pub(crate) tail: Option<u64>,
    pub(crate) since: Option<&'a str>,
    pub(crate) until: Option<&'a str>,
    pub(crate) timestamps: bool,
    pub(crate) prefix: bool,
    pub(crate) access: bool,
    pub(crate) json: bool,
}

pub(crate) fn handle_logs(config: &config::Config, options: HandleLogsOptions<'_>) -> Result<()> {
    if options.json && docker::is_dry_run() {
        anyhow::bail!(
            "`logs --json` streams container output and cannot be combined with --dry-run"
        );
    }
    if options.access {
        if options.json {
            anyhow::bail!("`logs --access --json` is not supported; drop one of them");
        }
        return cli::support::tail_access_logs(options.follow, options.tail);
    }

    let selected = super::service_scope::selected_services_in_scope(
        config,
        options.service,
        options.services,
        options.kind,
        options.profile,
    )?;
    if options.all {
        let cloned: Vec<config::ServiceConfig> = selected.iter().copied().cloned().collect();
        return docker::logs_many(
            &cloned,
            docker_logs_options(
                options.follow,
                options.tail,
                options.since,
                options.until,
                options.timestamps,
                options.prefix,
                options.json,
            ),
        );
    }

    let svc = if selected.len() > 1 {
        let cloned: Vec<config::ServiceConfig> = selected.iter().copied().cloned().collect();
        return docker::logs_many(
            &cloned,
            docker_logs_options(
                options.follow,
                options.tail,
                options.since,
                options.until,
                options.timestamps,
                options.prefix,
                options.json,
            ),
        );
    } else if let Some(first) = selected.first() {
        *first
    } else {
        anyhow::bail!("no services matched the requested selector")
    };
    run_single_service_logs(
        svc,
        docker_logs_options(
            options.follow,
            options.tail,
            options.since,
            options.until,
            options.timestamps,
            options.prefix,
            options.json,
        ),
    )
}

fn run_single_service_logs(
    service: &config::ServiceConfig,
    log_options: docker::LogsOptions,
) -> Result<()> {
    if log_options.prefix {
        docker::logs_prefixed(service, log_options)
    } else {
        docker::logs(service, log_options)
    }
}

fn docker_logs_options(
    follow: bool,
    tail: Option<u64>,
    since: Option<&str>,
    until: Option<&str>,
    timestamps: bool,
    prefix: bool,
    json: bool,
) -> docker::LogsOptions {
    docker::LogsOptions {
        follow,
        tail,
        since: since.map(ToOwned::to_owned),
        until: until.map(ToOwned::to_owned),
        timestamps,
        prefix,
        json,
    }
}
