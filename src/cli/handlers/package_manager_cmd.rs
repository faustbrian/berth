//! Shared package-manager command execution helper.
//!
//! Contains common logic used by `composer` and `node` handlers.

use anyhow::{Context, Result};
use std::path::Path;

use super::service_scope::selected_services_in_scope;
use crate::javascript::{
    BuildNodeCommandOptions, JavaScriptRuntime, JavaScriptToolchain, PackageManager,
    ResolveJavaScriptRuntimeOptions, VersionManager, build_node_command,
    resolve_javascript_runtime,
};
use crate::{cli, config};

pub(crate) struct HandlePackageManagerCommandOptions<'a> {
    pub(crate) service: Option<&'a str>,
    pub(crate) kind: Option<config::Kind>,
    pub(crate) profile: Option<&'a str>,
    pub(crate) command_bin: Option<&'a str>,
    pub(crate) runtime: Option<JavaScriptRuntime>,
    pub(crate) package_manager: Option<PackageManager>,
    pub(crate) version_manager: Option<VersionManager>,
    pub(crate) node_version: Option<&'a str>,
    pub(crate) non_interactive: bool,
    pub(crate) tty: bool,
    pub(crate) no_tty: bool,
    pub(crate) command: &'a [String],
    pub(crate) config_path: Option<&'a Path>,
    pub(crate) project_root: Option<&'a Path>,
    pub(crate) default_command: &'a [&'a str],
}

pub(crate) fn handle_package_manager_command(
    config: &config::Config,
    options: HandlePackageManagerCommandOptions<'_>,
) -> Result<()> {
    let selected_service =
        resolve_single_app_service(config, options.service, options.kind, options.profile)?;
    let runtime = cli::support::resolve_app_runtime_context(
        config,
        selected_service.as_deref(),
        options.config_path,
        options.project_root,
    )?;
    let command = resolve_package_manager_command(options.command, options.default_command);
    let node_runtime = resolve_javascript_runtime(ResolveJavaScriptRuntimeOptions {
        configured: runtime.target.javascript.as_ref(),
        workspace_root: runtime.workspace_root.as_path(),
        runtime: options.runtime,
        package_manager: options.package_manager,
        version_manager: options.version_manager,
        node_version: options.node_version,
        require_package_manager: options.command_bin.is_none(),
    })?;
    let mut target = runtime.target.clone();
    target.javascript = Some(JavaScriptToolchain {
        runtime: Some(node_runtime.runtime),
        package_manager: node_runtime.package_manager,
        version_manager: Some(node_runtime.version_manager),
        version: node_runtime.node_version.clone(),
    });
    let full_command = if let Some(command_bin) = options.command_bin {
        let mut full_command = vec![command_bin.to_owned()];
        full_command.extend(command);
        full_command
    } else {
        let mut base_command = node_runtime
            .package_manager
            .context("node package manager required")?
            .command_prefix();
        base_command.extend(command);
        build_node_command(BuildNodeCommandOptions {
            version_manager: node_runtime.version_manager,
            node_version: node_runtime.node_version.as_deref(),
            command: &base_command,
        })?
    };
    let tty = if options.non_interactive {
        false
    } else {
        cli::support::effective_tty(options.tty, options.no_tty)
    };
    let start_context = runtime.service_start_context();

    crate::docker::pass_through_exit_code(cli::support::run_service_command_with_tty(
        &target,
        &full_command,
        tty,
        &start_context,
    ))
}

fn resolve_package_manager_command(command: &[String], default_command: &[&str]) -> Vec<String> {
    if !command.is_empty() {
        return command.to_vec();
    }

    default_command
        .iter()
        .map(|part| (*part).to_owned())
        .collect()
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
        anyhow::bail!("no app services matched the requested selector")
    }
    if selected.len() > 1 {
        anyhow::bail!("selector matched multiple app services; use --service to choose one")
    }
    Ok(selected.pop().map(|svc| svc.name.clone()))
}

#[cfg(test)]
mod tests {
    use super::resolve_package_manager_command;

    #[test]
    fn resolve_package_manager_command_prefers_user_command() {
        let command = vec!["install".to_owned(), "--ansi".to_owned()];
        assert_eq!(
            resolve_package_manager_command(&command, &["list"]),
            command
        );
    }

    #[test]
    fn resolve_package_manager_command_uses_default_when_empty() {
        assert_eq!(
            resolve_package_manager_command(&[], &["list"]),
            vec!["list".to_owned()]
        );
    }

    #[test]
    fn resolve_package_manager_command_allows_empty_default() {
        assert!(resolve_package_manager_command(&[], &[]).is_empty());
    }
}
