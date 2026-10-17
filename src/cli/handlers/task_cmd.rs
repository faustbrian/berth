//! cli handlers task cmd module.
//!
//! Contains task command handling for opinionated workspace workflows.

use anyhow::{Context, Result};
use serde_json::Value;
use std::path::Path;

use crate::cli;
use crate::cli::args::{PackageManagerArg, VersionManagerArg};
use crate::config;
use crate::javascript::{
    BuildNodeCommandOptions, JavaScriptRuntime, ResolveJavaScriptRuntimeOptions,
    build_node_command, resolve_javascript_runtime,
};
use crate::output::{self, LogLevel, Persistence::Persistent};

pub(crate) struct HandleTaskDepsWorkflowOptions<'a> {
    pub(crate) action: TaskDependencyAction,
    pub(crate) service: Option<&'a str>,
    pub(crate) kind: Option<config::Kind>,
    pub(crate) profile: Option<&'a str>,
    pub(crate) composer: bool,
    pub(crate) node: bool,
    pub(crate) bun: bool,
    pub(crate) deno: bool,
    pub(crate) all: bool,
    pub(crate) package_manager: Option<PackageManagerArg>,
    pub(crate) version_manager: Option<VersionManagerArg>,
    pub(crate) node_version: Option<&'a str>,
    pub(crate) non_interactive: bool,
    pub(crate) quiet: bool,
    pub(crate) tty: bool,
    pub(crate) no_tty: bool,
    pub(crate) config_path: Option<&'a Path>,
    pub(crate) project_root: Option<&'a Path>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TaskDependencyAction {
    Bump,
    Audit,
    Normalize,
    Install,
}

impl TaskDependencyAction {
    const fn progress_verb(self) -> &'static str {
        match self {
            Self::Bump => "Bumping",
            Self::Audit => "Auditing",
            Self::Normalize => "Normalizing",
            Self::Install => "Installing",
        }
    }

    const fn completion_phrase(self) -> &'static str {
        match self {
            Self::Bump => "updated",
            Self::Audit => "audit completed",
            Self::Normalize => "normalized",
            Self::Install => "installed",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct TaskDependencyTargets {
    composer: bool,
    node: bool,
    bun: bool,
    deno: bool,
}

pub(crate) fn handle_task_deps_workflow(
    config: &config::Config,
    options: HandleTaskDepsWorkflowOptions<'_>,
) -> Result<()> {
    let targets = resolve_task_targets(
        options.action,
        options.composer,
        options.node,
        options.bun,
        options.deno,
        options.all,
    )?;
    let runtime = cli::support::resolve_app_runtime_context(
        config,
        resolve_single_app_service_name(config, options.service, options.kind, options.profile)?
            .as_deref(),
        options.config_path,
        options.project_root,
    )?;
    let tty = if options.non_interactive {
        false
    } else {
        cli::support::effective_tty(options.tty, options.no_tty)
    };
    let start_context = runtime.service_start_context();
    let workspace_root = runtime.workspace_root.as_path();

    if targets.composer {
        run_composer_workflow(
            workspace_root,
            runtime.target,
            &start_context,
            tty,
            options.quiet,
            options.action,
        )?;
    }

    if targets.node {
        run_node_workflow(
            workspace_root,
            runtime.target,
            &start_context,
            tty,
            options.quiet,
            options.action,
            options.package_manager,
            options.version_manager,
            options.node_version,
        )?;
    }

    if targets.bun {
        run_runtime_bump(
            RuntimeBumpOptions {
                workspace_root,
                target: runtime.target,
                start_context: &start_context,
                tty,
                quiet: options.quiet,
                runtime: JavaScriptRuntime::Bun,
                package_manager: None,
                version_manager: None,
                node_version: None,
                action: options.action,
            },
            RuntimeManifest::PackageJson {
                workflow_name: "Bun",
            },
        )?;
    }

    if targets.deno {
        run_runtime_bump(
            RuntimeBumpOptions {
                workspace_root,
                target: runtime.target,
                start_context: &start_context,
                tty,
                quiet: options.quiet,
                runtime: JavaScriptRuntime::Deno,
                package_manager: None,
                version_manager: None,
                node_version: None,
                action: options.action,
            },
            RuntimeManifest::Deno,
        )?;
    }

    Ok(())
}

fn resolve_single_app_service_name(
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
        anyhow::bail!("no app services matched the requested selector");
    }

    if selected.len() > 1 {
        anyhow::bail!("selector matched multiple app services; use --service to choose one");
    }

    Ok(selected
        .pop()
        .map(|service_config| service_config.name.clone()))
}

fn run_composer_workflow(
    workspace_root: &Path,
    target: &config::ServiceConfig,
    start_context: &cli::support::ServiceStartContext<'_>,
    tty: bool,
    quiet: bool,
    action: TaskDependencyAction,
) -> Result<()> {
    let manifest_path = workspace_root.join("composer.json");
    if !manifest_path.is_file() {
        output::event(
            "task",
            LogLevel::Warn,
            &format!(
                "Skipping Composer bump: no composer.json in {}",
                workspace_root.display()
            ),
            Persistent,
        );
        return Ok(());
    }

    let package_name = read_package_name(&manifest_path).with_context(|| {
        format!(
            "failed to read package name from {}",
            manifest_path.display()
        )
    })?;

    super::log::info_if_not_quiet(
        quiet,
        "task",
        &format!(
            "{} Composer dependencies for {package_name}",
            action.progress_verb()
        ),
    );

    for command in composer_commands(action) {
        let display = command.join(" ");
        crate::docker::pass_through_exit_code(cli::support::run_service_command_with_tty(
            target,
            &command,
            tty,
            start_context,
        ))
        .with_context(|| format!("Composer workflow step failed for {package_name}: {display}"))?;
    }

    super::log::success_if_not_quiet(
        quiet,
        "task",
        &format!(
            "Composer dependencies {} for {package_name}",
            action.completion_phrase()
        ),
    );

    Ok(())
}

fn run_node_workflow(
    workspace_root: &Path,
    target: &config::ServiceConfig,
    start_context: &cli::support::ServiceStartContext<'_>,
    tty: bool,
    quiet: bool,
    action: TaskDependencyAction,
    requested_package_manager: Option<PackageManagerArg>,
    requested_version_manager: Option<VersionManagerArg>,
    requested_node_version: Option<&str>,
) -> Result<()> {
    let javascript_runtime = resolve_javascript_runtime(ResolveJavaScriptRuntimeOptions {
        configured: target.javascript.as_ref(),
        workspace_root,
        runtime: Some(JavaScriptRuntime::Node),
        package_manager: requested_package_manager,
        version_manager: requested_version_manager,
        node_version: requested_node_version,
        require_package_manager: true,
    })?;

    run_runtime_bump(
        RuntimeBumpOptions {
            workspace_root,
            target,
            start_context,
            tty,
            quiet,
            runtime: JavaScriptRuntime::Node,
            package_manager: javascript_runtime.package_manager,
            version_manager: Some(javascript_runtime.version_manager),
            node_version: javascript_runtime.node_version.as_deref(),
            action,
        },
        RuntimeManifest::PackageJson {
            workflow_name: "Node",
        },
    )
}

fn read_package_name(path: &Path) -> Result<String> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read manifest {}", path.display()))?;
    let parsed: Value = serde_json::from_str(&content)
        .with_context(|| format!("failed to parse manifest {}", path.display()))?;

    Ok(parsed
        .get("name")
        .and_then(Value::as_str)
        .map_or_else(|| "unknown".to_owned(), str::to_owned))
}

fn composer_commands(action: TaskDependencyAction) -> Vec<Vec<String>> {
    match action {
        TaskDependencyAction::Bump => vec![
            vec![
                "composer".to_owned(),
                "bump".to_owned(),
                "--dev-only".to_owned(),
            ],
            vec![
                "composer".to_owned(),
                "bump".to_owned(),
                "--no-dev-only".to_owned(),
            ],
            vec![
                "composer".to_owned(),
                "update".to_owned(),
                "--ignore-platform-reqs".to_owned(),
            ],
            vec!["composer".to_owned(), "normalize".to_owned()],
        ],
        TaskDependencyAction::Audit => {
            vec![vec!["composer".to_owned(), "audit".to_owned()]]
        }
        TaskDependencyAction::Normalize => {
            vec![vec!["composer".to_owned(), "normalize".to_owned()]]
        }
        TaskDependencyAction::Install => {
            vec![vec!["composer".to_owned(), "install".to_owned()]]
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DependencyWorkflow {
    Bun,
    Deno,
    Npm,
    Pnpm,
    Yarn,
}

impl DependencyWorkflow {
    fn commands(self, action: TaskDependencyAction) -> Option<Vec<Vec<String>>> {
        match (self, action) {
            (Self::Bun, TaskDependencyAction::Bump) => Some(vec![vec![
                "bun".to_owned(),
                "update".to_owned(),
                "--latest".to_owned(),
            ]]),
            (Self::Bun, TaskDependencyAction::Audit) => {
                Some(vec![vec!["bun".to_owned(), "audit".to_owned()]])
            }
            (Self::Bun, TaskDependencyAction::Normalize) => {
                Some(vec![vec!["bun".to_owned(), "install".to_owned()]])
            }
            (Self::Bun, TaskDependencyAction::Install) => {
                Some(vec![vec!["bun".to_owned(), "install".to_owned()]])
            }
            (Self::Deno, TaskDependencyAction::Bump) => Some(vec![vec![
                "deno".to_owned(),
                "outdated".to_owned(),
                "--update".to_owned(),
                "--latest".to_owned(),
            ]]),
            (Self::Npm, TaskDependencyAction::Bump) => Some(vec![
                vec![
                    "npx".to_owned(),
                    "--yes".to_owned(),
                    "npm-check-updates".to_owned(),
                    "-u".to_owned(),
                ],
                vec!["npm".to_owned(), "install".to_owned()],
            ]),
            (Self::Pnpm, TaskDependencyAction::Bump) => Some(vec![vec![
                "pnpm".to_owned(),
                "update".to_owned(),
                "--latest".to_owned(),
            ]]),
            (Self::Yarn, TaskDependencyAction::Bump) => Some(vec![vec![
                "yarn".to_owned(),
                "up".to_owned(),
                "*".to_owned(),
            ]]),
            (Self::Npm, TaskDependencyAction::Audit) => {
                Some(vec![vec!["npm".to_owned(), "audit".to_owned()]])
            }
            (Self::Pnpm, TaskDependencyAction::Audit) => {
                Some(vec![vec!["pnpm".to_owned(), "audit".to_owned()]])
            }
            (Self::Yarn, TaskDependencyAction::Audit) => Some(vec![vec![
                "yarn".to_owned(),
                "npm".to_owned(),
                "audit".to_owned(),
            ]]),
            (Self::Npm, TaskDependencyAction::Normalize) => Some(vec![vec![
                "npm".to_owned(),
                "install".to_owned(),
                "--package-lock-only".to_owned(),
            ]]),
            (Self::Pnpm, TaskDependencyAction::Normalize) => Some(vec![vec![
                "pnpm".to_owned(),
                "install".to_owned(),
                "--lockfile-only".to_owned(),
            ]]),
            (Self::Yarn, TaskDependencyAction::Normalize) => Some(vec![vec![
                "yarn".to_owned(),
                "install".to_owned(),
                "--mode".to_owned(),
                "skip-build".to_owned(),
            ]]),
            (Self::Npm, TaskDependencyAction::Install) => {
                Some(vec![vec!["npm".to_owned(), "install".to_owned()]])
            }
            (Self::Pnpm, TaskDependencyAction::Install) => {
                Some(vec![vec!["pnpm".to_owned(), "install".to_owned()]])
            }
            (Self::Yarn, TaskDependencyAction::Install) => {
                Some(vec![vec!["yarn".to_owned(), "install".to_owned()]])
            }
            _ => None,
        }
    }

    const fn display_name(self) -> &'static str {
        match self {
            Self::Bun => "bun",
            Self::Deno => "deno",
            Self::Npm => "npm",
            Self::Pnpm => "pnpm",
            Self::Yarn => "yarn",
        }
    }

    const fn wrap_with_node_manager(self) -> bool {
        matches!(self, Self::Npm | Self::Pnpm | Self::Yarn)
    }
}

fn resolve_workflow(
    runtime: JavaScriptRuntime,
    package_manager: Option<PackageManagerArg>,
) -> Result<DependencyWorkflow> {
    match runtime {
        JavaScriptRuntime::Bun => Ok(DependencyWorkflow::Bun),
        JavaScriptRuntime::Deno => Ok(DependencyWorkflow::Deno),
        JavaScriptRuntime::Node => package_manager
            .map(|manager| match manager {
                PackageManagerArg::Npm => DependencyWorkflow::Npm,
                PackageManagerArg::Pnpm => DependencyWorkflow::Pnpm,
                PackageManagerArg::Yarn => DependencyWorkflow::Yarn,
            })
            .context("node package manager required"),
    }
}

struct RuntimeBumpOptions<'a> {
    workspace_root: &'a Path,
    target: &'a config::ServiceConfig,
    start_context: &'a cli::support::ServiceStartContext<'a>,
    tty: bool,
    quiet: bool,
    runtime: JavaScriptRuntime,
    package_manager: Option<PackageManagerArg>,
    version_manager: Option<VersionManagerArg>,
    node_version: Option<&'a str>,
    action: TaskDependencyAction,
}

enum RuntimeManifest<'a> {
    PackageJson { workflow_name: &'a str },
    Deno,
}

fn run_runtime_bump(options: RuntimeBumpOptions<'_>, manifest: RuntimeManifest<'_>) -> Result<()> {
    let workflow = resolve_workflow(options.runtime, options.package_manager)?;
    let package_name = match manifest {
        RuntimeManifest::PackageJson { workflow_name } => {
            let manifest_path = options.workspace_root.join("package.json");
            if !manifest_path.is_file() {
                output::event(
                    "task",
                    LogLevel::Warn,
                    &format!(
                        "Skipping {workflow_name} bump: no package.json in {}",
                        options.workspace_root.display()
                    ),
                    Persistent,
                );
                return Ok(());
            }

            read_package_name(&manifest_path).with_context(|| {
                format!(
                    "failed to read package name from {}",
                    manifest_path.display()
                )
            })?
        }
        RuntimeManifest::Deno => {
            if !has_deno_project(options.workspace_root) {
                output::event(
                    "task",
                    LogLevel::Warn,
                    &format!(
                        "Skipping Deno bump: no deno.json, deno.jsonc, or deno.lock in {}",
                        options.workspace_root.display()
                    ),
                    Persistent,
                );
                return Ok(());
            }

            options
                .workspace_root
                .file_name()
                .and_then(|name| name.to_str())
                .map(str::to_owned)
                .unwrap_or_else(|| "unknown".to_owned())
        }
    };

    super::log::info_if_not_quiet(
        options.quiet,
        "task",
        &format!(
            "{} {} dependencies for {package_name}",
            options.action.progress_verb(),
            workflow.display_name()
        ),
    );

    let Some(commands) = workflow.commands(options.action) else {
        output::event(
            "task",
            LogLevel::Warn,
            &format!(
                "Skipping {} workflow for {}: runtime does not support this action",
                workflow.display_name(),
                action_name(options.action)
            ),
            Persistent,
        );
        return Ok(());
    };

    for command in commands {
        let wrapped = if workflow.wrap_with_node_manager() {
            build_node_command(BuildNodeCommandOptions {
                version_manager: options
                    .version_manager
                    .context("node version manager required")?,
                node_version: options.node_version,
                command: &command,
            })?
        } else {
            command
        };
        let display = wrapped.join(" ");
        crate::docker::pass_through_exit_code(cli::support::run_service_command_with_tty(
            options.target,
            &wrapped,
            options.tty,
            options.start_context,
        ))
        .with_context(|| {
            format!("Dependency workflow step failed for {package_name}: {display}")
        })?;
    }

    super::log::success_if_not_quiet(
        options.quiet,
        "task",
        &format!(
            "{} dependencies {} for {package_name}",
            workflow.display_name(),
            options.action.completion_phrase()
        ),
    );

    Ok(())
}

fn has_deno_project(workspace_root: &Path) -> bool {
    ["deno.json", "deno.jsonc", "deno.lock"]
        .into_iter()
        .any(|name| workspace_root.join(name).is_file())
}

fn resolve_task_targets(
    action: TaskDependencyAction,
    composer: bool,
    node: bool,
    bun: bool,
    deno: bool,
    all: bool,
) -> Result<TaskDependencyTargets> {
    if all {
        return Ok(TaskDependencyTargets {
            composer: true,
            node: true,
            bun: true,
            deno: true,
        });
    }

    if composer || node || bun || deno {
        return Ok(TaskDependencyTargets {
            composer,
            node,
            bun,
            deno,
        });
    }

    let _ = action;
    anyhow::bail!("select at least one target with --composer, --node, --bun, --deno, or --all")
}

fn action_name(action: TaskDependencyAction) -> &'static str {
    match action {
        TaskDependencyAction::Bump => "bump",
        TaskDependencyAction::Audit => "audit",
        TaskDependencyAction::Normalize => "normalize",
        TaskDependencyAction::Install => "install",
    }
}

#[cfg(test)]
mod tests {
    use super::{
        DependencyWorkflow, TaskDependencyAction, composer_commands, read_package_name,
        resolve_task_targets, resolve_workflow,
    };
    use crate::cli::args::PackageManagerArg;
    use crate::javascript::{JavaScriptRuntime, detect_node_package_manager};
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_root(prefix: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "{prefix}-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        drop(fs::remove_dir_all(&root));
        fs::create_dir_all(&root).expect("create temp root");
        root
    }

    #[test]
    fn resolve_task_targets_accepts_explicit_flags() {
        let targets =
            resolve_task_targets(TaskDependencyAction::Bump, true, false, false, false, false)
                .expect("composer target");
        assert!(targets.composer);
        assert!(!targets.node);
        assert!(!targets.bun);
        assert!(!targets.deno);

        let targets = resolve_task_targets(
            TaskDependencyAction::Audit,
            false,
            true,
            false,
            false,
            false,
        )
        .expect("node target");
        assert!(!targets.composer);
        assert!(targets.node);
        assert!(!targets.bun);
        assert!(!targets.deno);

        let targets = resolve_task_targets(
            TaskDependencyAction::Normalize,
            false,
            false,
            true,
            false,
            false,
        )
        .expect("bun target");
        assert!(!targets.composer);
        assert!(!targets.node);
        assert!(targets.bun);
        assert!(!targets.deno);

        let targets = resolve_task_targets(
            TaskDependencyAction::Install,
            false,
            false,
            false,
            true,
            false,
        )
        .expect("deno target");
        assert!(!targets.composer);
        assert!(!targets.node);
        assert!(!targets.bun);
        assert!(targets.deno);

        let targets = resolve_task_targets(
            TaskDependencyAction::Install,
            false,
            false,
            false,
            false,
            true,
        )
        .expect("all targets");
        assert!(targets.composer);
        assert!(targets.node);
        assert!(targets.bun);
        assert!(targets.deno);
    }

    #[test]
    fn resolve_task_targets_requires_a_target_flag() {
        let error = resolve_task_targets(
            TaskDependencyAction::Bump,
            false,
            false,
            false,
            false,
            false,
        )
        .expect_err("missing target");
        assert!(error.to_string().contains("select at least one target"));
    }

    #[test]
    fn detect_node_manager_prefers_matching_lockfile() {
        let root = temp_root("berth-task-node-manager");
        fs::write(root.join("pnpm-lock.yaml"), "lockfileVersion: '9.0'").expect("lockfile");

        let manager = detect_node_package_manager(&root);
        assert!(matches!(manager, Some(PackageManagerArg::Pnpm)));

        fs::remove_dir_all(&root).expect("cleanup");
    }

    #[test]
    fn detect_node_manager_reads_package_manager_from_package_json() {
        let root = temp_root("berth-task-node-package-manager");
        fs::write(
            root.join("package.json"),
            r#"{"name":"demo","packageManager":"yarn@4.6.0"}"#,
        )
        .expect("package json");

        let manager = detect_node_package_manager(&root);
        assert!(matches!(manager, Some(PackageManagerArg::Yarn)));

        fs::remove_dir_all(&root).expect("cleanup");
    }

    #[test]
    fn read_package_name_defaults_to_unknown_without_name() {
        let root = temp_root("berth-task-read-package-name");
        let manifest = root.join("composer.json");
        fs::write(&manifest, "{}").expect("write manifest");

        let package_name = read_package_name(&manifest).expect("package name");
        assert_eq!(package_name, "unknown");

        fs::remove_dir_all(&root).expect("cleanup");
    }

    #[test]
    fn composer_commands_match_supported_workflows() {
        assert_eq!(
            composer_commands(TaskDependencyAction::Bump),
            vec![
                vec![
                    "composer".to_owned(),
                    "bump".to_owned(),
                    "--dev-only".to_owned(),
                ],
                vec![
                    "composer".to_owned(),
                    "bump".to_owned(),
                    "--no-dev-only".to_owned(),
                ],
                vec![
                    "composer".to_owned(),
                    "update".to_owned(),
                    "--ignore-platform-reqs".to_owned(),
                ],
                vec!["composer".to_owned(), "normalize".to_owned()],
            ]
        );
        assert_eq!(
            composer_commands(TaskDependencyAction::Audit),
            vec![vec!["composer".to_owned(), "audit".to_owned()]]
        );
        assert_eq!(
            composer_commands(TaskDependencyAction::Normalize),
            vec![vec!["composer".to_owned(), "normalize".to_owned()]]
        );
        assert_eq!(
            composer_commands(TaskDependencyAction::Install),
            vec![vec!["composer".to_owned(), "install".to_owned()]]
        );
    }

    #[test]
    fn runtime_commands_match_supported_workflows() {
        assert_eq!(
            DependencyWorkflow::Bun.commands(TaskDependencyAction::Bump),
            Some(vec![vec![
                "bun".to_owned(),
                "update".to_owned(),
                "--latest".to_owned(),
            ]])
        );
        assert_eq!(
            DependencyWorkflow::Bun.commands(TaskDependencyAction::Audit),
            Some(vec![vec!["bun".to_owned(), "audit".to_owned(),]])
        );
        assert_eq!(
            DependencyWorkflow::Npm.commands(TaskDependencyAction::Bump),
            Some(vec![
                vec![
                    "npx".to_owned(),
                    "--yes".to_owned(),
                    "npm-check-updates".to_owned(),
                    "-u".to_owned(),
                ],
                vec!["npm".to_owned(), "install".to_owned()],
            ])
        );
        assert_eq!(
            DependencyWorkflow::Pnpm.commands(TaskDependencyAction::Normalize),
            Some(vec![vec![
                "pnpm".to_owned(),
                "install".to_owned(),
                "--lockfile-only".to_owned(),
            ]])
        );
        assert_eq!(
            DependencyWorkflow::Yarn.commands(TaskDependencyAction::Install),
            Some(vec![vec!["yarn".to_owned(), "install".to_owned()]])
        );
        assert_eq!(
            DependencyWorkflow::Deno.commands(TaskDependencyAction::Bump),
            Some(vec![vec![
                "deno".to_owned(),
                "outdated".to_owned(),
                "--update".to_owned(),
                "--latest".to_owned(),
            ]])
        );
        assert_eq!(
            DependencyWorkflow::Deno.commands(TaskDependencyAction::Audit),
            None
        );
        assert_eq!(
            DependencyWorkflow::Pnpm.commands(TaskDependencyAction::Bump),
            Some(vec![vec![
                "pnpm".to_owned(),
                "update".to_owned(),
                "--latest".to_owned(),
            ]])
        );
    }

    #[test]
    fn resolve_workflow_uses_bun_runtime_directly() {
        let workflow = resolve_workflow(JavaScriptRuntime::Bun, None).expect("bun workflow");
        assert_eq!(workflow, DependencyWorkflow::Bun);
        assert!(!workflow.wrap_with_node_manager());
    }

    #[test]
    fn resolve_workflow_uses_deno_runtime_directly() {
        let workflow = resolve_workflow(JavaScriptRuntime::Deno, None).expect("deno workflow");
        assert_eq!(workflow, DependencyWorkflow::Deno);
        assert!(!workflow.wrap_with_node_manager());
    }
}
