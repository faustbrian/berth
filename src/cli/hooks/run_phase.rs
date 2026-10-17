//! Lifecycle hook execution for selected services.

use anyhow::Result;
use std::path::Path;

use crate::config::{HookPhase, ServiceConfig};

mod execute;
mod run_script;
mod runner;
use execute::{execute_hook, hook_phase_label};
use run_script::run_script_hook;
use runner::run_phase_hooks_with_executor;

pub(crate) fn run_phase_hooks_for_services(
    services: &[&ServiceConfig],
    phase: HookPhase,
    workspace_root: &Path,
    quiet: bool,
) -> Result<()> {
    run_phase_hooks_with_executor(services, phase, workspace_root, quiet, execute_hook)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Driver, HookOnError, HookRun, Kind, ServiceHook};

    fn app_service_with_hooks(hook: Vec<ServiceHook>) -> ServiceConfig {
        ServiceConfig {
            name: "app".to_owned(),
            kind: Kind::App,
            driver: Driver::Frankenphp,
            image: "dunglas/frankenphp:php8.5".to_owned(),
            host: "127.0.0.1".to_owned(),
            port: 8000,
            database: None,
            username: None,
            password: None,
            bucket: None,
            access_key: None,
            secret_key: None,
            api_key: None,
            region: None,
            scheme: None,
            domain: Some("app.localhost".to_owned()),
            domains: None,
            resolved_domain: None,
            container_port: None,
            smtp_port: None,
            volumes: None,
            env: None,
            command: None,
            depends_on: None,
            seed_file: None,
            hook,
            health_path: None,
            health_statuses: None,
            restart: None,
            localhost_tls: false,
            octane: false,
            octane_workers: None,
            octane_max_requests: None,
            php_extensions: None,
            trust_container_ca: false,
            env_mapping: None,
            javascript: None,
            container_name: Some("test-app".to_owned()),
            resolved_container_name: None,
            runtime: Default::default(),
        }
    }

    #[test]
    fn run_phase_executes_only_matching_hooks() {
        let service = app_service_with_hooks(vec![
            ServiceHook {
                name: "one".to_owned(),
                phase: HookPhase::PostUp,
                run: HookRun::Exec {
                    argv: vec!["php".to_owned(), "-v".to_owned()],
                },
                on_error: HookOnError::Fail,
                timeout_sec: None,
            },
            ServiceHook {
                name: "two".to_owned(),
                phase: HookPhase::PostDown,
                run: HookRun::Script {
                    path: ".berth/hooks/two.sh".to_owned(),
                },
                on_error: HookOnError::Fail,
                timeout_sec: None,
            },
        ]);
        let selected = vec![&service];
        let mut executed = Vec::new();

        run_phase_hooks_with_executor(
            &selected,
            HookPhase::PostUp,
            Path::new("/tmp/work"),
            true,
            |_, hook, _, _| {
                executed.push(hook.name.clone());
                Ok(())
            },
        )
        .expect("run post_up hooks");

        assert_eq!(executed, vec!["one".to_owned()]);
    }

    #[test]
    fn run_phase_warns_and_continues_when_on_error_warn() {
        let service = app_service_with_hooks(vec![
            ServiceHook {
                name: "warn-hook".to_owned(),
                phase: HookPhase::PostUp,
                run: HookRun::Exec {
                    argv: vec!["php".to_owned(), "-v".to_owned()],
                },
                on_error: HookOnError::Warn,
                timeout_sec: None,
            },
            ServiceHook {
                name: "next".to_owned(),
                phase: HookPhase::PostUp,
                run: HookRun::Exec {
                    argv: vec!["php".to_owned(), "-m".to_owned()],
                },
                on_error: HookOnError::Fail,
                timeout_sec: None,
            },
        ]);
        let selected = vec![&service];
        let mut executed = Vec::new();

        run_phase_hooks_with_executor(
            &selected,
            HookPhase::PostUp,
            Path::new("/tmp/work"),
            true,
            |_, hook, _, _| {
                executed.push(hook.name.clone());
                if hook.name == "warn-hook" {
                    anyhow::bail!("boom");
                }
                Ok(())
            },
        )
        .expect("warn hook should not fail command");

        assert_eq!(executed, vec!["warn-hook".to_owned(), "next".to_owned()]);
    }

    #[test]
    fn run_phase_fails_when_on_error_fail() {
        let service = app_service_with_hooks(vec![
            ServiceHook {
                name: "fail-hook".to_owned(),
                phase: HookPhase::PostUp,
                run: HookRun::Exec {
                    argv: vec!["php".to_owned(), "-v".to_owned()],
                },
                on_error: HookOnError::Fail,
                timeout_sec: None,
            },
            ServiceHook {
                name: "next".to_owned(),
                phase: HookPhase::PostUp,
                run: HookRun::Exec {
                    argv: vec!["php".to_owned(), "-m".to_owned()],
                },
                on_error: HookOnError::Fail,
                timeout_sec: None,
            },
        ]);
        let selected = vec![&service];
        let mut executed = Vec::new();

        let error = run_phase_hooks_with_executor(
            &selected,
            HookPhase::PostUp,
            Path::new("/tmp/work"),
            true,
            |_, hook, _, _| {
                executed.push(hook.name.clone());
                anyhow::bail!("boom");
            },
        )
        .expect_err("fail hook should stop command");

        assert!(error.to_string().contains("hook 'fail-hook' failed"));
        assert_eq!(executed, vec!["fail-hook".to_owned()]);
    }

    #[test]
    fn resolve_script_path_joins_workspace_root_for_relative_paths() {
        let root = Path::new("/tmp/workspace");
        let resolved = run_script::resolve_script_path(".berth/hooks/seed.sh", root);
        assert_eq!(
            resolved,
            std::path::PathBuf::from("/tmp/workspace/.berth/hooks/seed.sh")
        );
    }
}
