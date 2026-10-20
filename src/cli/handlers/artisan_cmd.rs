//! cli handlers artisan cmd module.
//!
//! Contains cli handlers artisan cmd logic used by Berth command workflows.

use anyhow::{Result, anyhow};
use std::path::Path;

use super::service_scope::selected_services_in_scope;
use crate::{cli, config};

mod command;
mod runtime;
mod test_command;
mod test_extensions;
#[cfg(test)]
mod tests;

use command::{
    build_artisan_command, ensure_artisan_subcommand, is_artisan_test_command,
    remove_artisan_env_overrides, resolve_artisan_tty,
};
use runtime::{acquire_testing_runtime_lease, cleanup_test_services, ensure_test_services_running};
use test_command::{build_artisan_test_command, should_bootstrap_playwright};
use test_extensions::ensure_artisan_test_runtime_extensions;

pub(crate) struct HandleArtisanOptions<'a> {
    pub(crate) service: Option<&'a str>,
    pub(crate) kind: Option<config::Kind>,
    pub(crate) profile: Option<&'a str>,
    pub(crate) non_interactive: bool,
    pub(crate) browser: bool,
    pub(crate) tty: bool,
    pub(crate) no_tty: bool,
    pub(crate) command: &'a [String],
    pub(crate) config_path: Option<&'a Path>,
    pub(crate) project_root: Option<&'a Path>,
}

pub(crate) fn handle_artisan(
    config: &config::Config,
    options: HandleArtisanOptions<'_>,
) -> Result<()> {
    if config.project_type == config::ProjectType::Library {
        anyhow::bail!("`berth artisan` is only supported when project_type is \"project\"");
    }

    let browser_requested = options.browser
        || options
            .command
            .iter()
            .any(|arg| arg == "--browser" || arg.starts_with("--browser="));
    let mut user_command: Vec<String> = options
        .command
        .iter()
        .filter(|arg| arg.as_str() != "--browser" && !arg.starts_with("--browser="))
        .cloned()
        .collect();

    user_command = ensure_artisan_subcommand(user_command);
    let is_test_command = is_artisan_test_command(&user_command);
    let mut effective_config = config.clone();
    let mut _testing_runtime_lease = None;
    let mut workspace_root = None;
    let mut bootstrap_playwright = false;
    if is_test_command {
        let resolved_workspace_root =
            cli::support::workspace_root(options.config_path, options.project_root)?;
        bootstrap_playwright =
            browser_requested && should_bootstrap_playwright(&resolved_workspace_root);
        effective_config = load_artisan_test_base_config(
            options.config_path,
            options.project_root,
            &resolved_workspace_root,
        )?;
        let lease = acquire_testing_runtime_lease(&resolved_workspace_root)?;
        let runtime_env = lease.runtime_env_name().to_owned();
        _testing_runtime_lease = Some(lease);
        workspace_root = Some(resolved_workspace_root);
        config::apply_runtime_env(&mut effective_config, &runtime_env)?;
        user_command = remove_artisan_env_overrides(&user_command);
        user_command.push("--env=testing".to_owned());
    }
    let selected_service = resolve_single_app_service(
        &effective_config,
        options.service,
        options.kind,
        options.profile,
    )?;
    if is_test_command {
        let selected_app =
            config::resolve_app_service(&effective_config, selected_service.as_deref())?
                .name
                .clone();
        let target = effective_config
            .service
            .iter_mut()
            .find(|service| service.name == selected_app)
            .ok_or_else(|| anyhow!("app service '{selected_app}' should be configured"))?;
        ensure_artisan_test_runtime_extensions(target);
    }

    let mut prepared_test_runtime = false;
    if is_test_command {
        let resolved_workspace_root = workspace_root
            .clone()
            .ok_or_else(|| anyhow!("testing workspace root should be set"))?;
        match ensure_test_services_running(
            &mut effective_config,
            &resolved_workspace_root,
            selected_service.as_deref(),
            bootstrap_playwright,
        ) {
            Ok(()) => prepared_test_runtime = true,
            Err(start_error) => {
                return match cleanup_test_services(&effective_config, selected_service.as_deref()) {
                    Ok(()) => Err(start_error),
                    Err(cleanup_error) => Err(anyhow!(
                        "failed to prepare test runtime: {start_error}; failed to cleanup test \
                         runtime: {cleanup_error}"
                    )),
                };
            }
        }
        workspace_root = Some(resolved_workspace_root);
    }
    let runtime = if let Some(ref workspace_root) = workspace_root {
        cli::support::resolve_artisan_test_runtime_context(
            &effective_config,
            selected_service.as_deref(),
            workspace_root.clone(),
        )?
    } else {
        cli::support::resolve_app_runtime_context(
            &effective_config,
            selected_service.as_deref(),
            options.config_path,
            options.project_root,
        )?
    };
    let full_command = if is_test_command {
        build_artisan_test_command(user_command, &runtime.app_env, bootstrap_playwright)
    } else {
        build_artisan_command(user_command)
    };
    let start_context = runtime.service_start_context();
    let command_result =
        crate::docker::pass_through_exit_code(cli::support::run_service_command_with_tty(
            runtime.target,
            &full_command,
            if options.non_interactive {
                false
            } else {
                resolve_artisan_tty(options.tty, options.no_tty)
            },
            &start_context,
        ));

    if !(is_test_command && prepared_test_runtime) {
        return command_result;
    }

    let cleanup_result = cleanup_test_services(&effective_config, selected_service.as_deref());
    match (command_result, cleanup_result) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(command_error), Ok(())) => Err(command_error),
        (Ok(()), Err(cleanup_error)) => Err(cleanup_error),
        // Keep the command's error (and its exit code) as the cause; the
        // cleanup failure is added as context.
        (Err(command_error), Err(cleanup_error)) => Err(command_error.context(format!(
            "artisan test failed and test runtime cleanup also failed: {cleanup_error}"
        ))),
    }
}

pub(crate) fn set_testing_runtime_pool_size_override(pool_size: Option<usize>) {
    runtime::set_testing_runtime_pool_size_override(pool_size);
}

fn load_artisan_test_base_config(
    config_path: Option<&Path>,
    project_root: Option<&Path>,
    workspace_root: &Path,
) -> Result<config::Config> {
    let load_root = project_root.or(Some(workspace_root));
    config::load_config_with(
        config::LoadConfigPathOptions::new(config_path, load_root)
            .with_runtime_env(Some("testing")),
    )
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
    let mut selected = selected_services_in_scope(config, service, &[], kind, profile)?
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
