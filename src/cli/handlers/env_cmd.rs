//! cli handlers env cmd module.
//!
//! Prints the environment berth injects into app containers. berth never
//! writes `.env`: `env generate --output` writes a new file the caller names.

use anyhow::Result;
use std::collections::{BTreeMap, HashMap};
use std::path::Path;

use crate::cli::args::EnvCommands;
use crate::{cli, config, env};

mod generate;

pub(crate) struct HandleEnvOptions<'a> {
    pub(crate) command: Option<&'a EnvCommands>,
    pub(crate) service: Option<&'a str>,
    pub(crate) json: bool,
    pub(crate) writes_dotenv: bool,
    pub(crate) quiet: bool,
    pub(crate) config_path: Option<&'a Path>,
    pub(crate) project_root: Option<&'a Path>,
}

pub(crate) fn handle_env(config: &mut config::Config, options: HandleEnvOptions<'_>) -> Result<()> {
    if options.writes_dotenv {
        anyhow::bail!("{}", cli::support::ENV_WRITE_REMOVED);
    }
    let views = env_views(
        config,
        options.service,
        options.config_path,
        options.project_root,
    )?;
    if let Some(EnvCommands::Generate { output, host }) = options.command {
        let values = if *host { &views.host } else { &views.container };
        return generate::handle_generate_env(output, values, options.quiet);
    }
    if options.json {
        let document = serde_json::json!({
            "environment": views.container,
            "host": views.host,
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&document)
                .map_err(|error| anyhow::anyhow!("failed to render env JSON: {error}"))?
        );
        return Ok(());
    }
    print!("{}", env::render_dotenv(&views.container));
    Ok(())
}

/// The environment an app container receives, and the subset of endpoint
/// values a host tool can use.
pub(crate) struct EnvViews {
    pub(crate) container: BTreeMap<String, String>,
    pub(crate) host: BTreeMap<String, String>,
}

/// Builds both views for the app service (`service`, or the project's app).
///
/// # Errors
///
/// Returns an error when the project's dependencies or managed values cannot
/// be resolved.
pub(crate) fn env_views(
    config: &config::Config,
    service: Option<&str>,
    config_path: Option<&Path>,
    project_root: Option<&Path>,
) -> Result<EnvViews> {
    let has_app = config
        .service
        .iter()
        .any(|svc| svc.kind == config::Kind::App);
    let container: HashMap<String, String> = if has_app {
        let runtime =
            cli::support::resolve_app_runtime_context(config, service, config_path, project_root)?;
        // Built by the same function as the container's `-e` arguments.
        crate::serve::app_container_env(runtime.target, &runtime.app_env)
            .into_iter()
            .collect()
    } else {
        cli::support::runtime_app_env(config, &HashMap::new())?
    };
    Ok(EnvViews {
        container: container.into_iter().collect(),
        host: env::host_env(config).into_iter().collect(),
    })
}
