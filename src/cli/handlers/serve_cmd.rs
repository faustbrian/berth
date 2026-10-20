//! cli handlers serve cmd module.
//!
//! Contains cli handlers serve cmd logic used by Berth command workflows.

use anyhow::Result;
use std::path::Path;

use crate::{cli, config, serve};

pub(crate) struct HandleServeOptions<'a> {
    pub(crate) service: Option<&'a str>,
    pub(crate) kind: Option<config::Kind>,
    pub(crate) profile: Option<&'a str>,
    pub(crate) recreate: bool,
    pub(crate) detached: bool,
    pub(crate) env_output: bool,
    pub(crate) trust_container_ca: bool,
    pub(crate) config_path: Option<&'a Path>,
    pub(crate) project_root: Option<&'a Path>,
}

pub(crate) fn handle_serve(config: &config::Config, options: HandleServeOptions<'_>) -> Result<()> {
    if options.env_output {
        anyhow::bail!("{}", cli::support::env_write_removed());
    }
    let selected_service =
        resolve_single_app_service(config, options.service, options.kind, options.profile)?;
    let runtime = cli::support::resolve_app_runtime_context(
        config,
        selected_service.as_deref(),
        options.config_path,
        options.project_root,
    )?;
    let effective_trust_container_ca =
        options.trust_container_ca || runtime.target.trust_container_ca;
    serve::run(serve::RunServeOptions {
        target: runtime.target,
        recreate: options.recreate,
        trust_container_ca: effective_trust_container_ca,
        detached: options.detached,
        project_root: &runtime.workspace_root,
        injected_env: &runtime.app_env,
        allow_rebuild: true,
    })
}

fn resolve_single_app_service(
    config: &config::Config,
    service: Option<&str>,
    kind: Option<config::Kind>,
    profile: Option<&str>,
) -> Result<Option<String>> {
    if service.is_none() && kind.is_none() && profile.is_none() {
        return Ok(None);
    }
    let mut selected =
        cli::support::selected_services_with_filters(config, service, &[], kind, None, profile)?
            .into_iter()
            .filter(|svc| svc.kind == config::Kind::App)
            .collect::<Vec<_>>();
    if selected.is_empty() {
        return Err(cli::support::nothing_matched(
            config, service, kind, profile, true,
        ));
    }
    if selected.len() > 1 {
        return Err(cli::support::several_matched(
            service,
            kind,
            profile,
            selected.iter().map(|svc| &**svc),
        ));
    }
    Ok(selected.pop().map(|svc| svc.name.clone()))
}
