//! Lifecycle hook execution helpers.

use anyhow::Result;
use std::path::Path;

use crate::config::{HookPhase, HookRun, ServiceConfig, ServiceHook};

use super::run_script_hook;

pub(super) fn execute_hook(
    service: &ServiceConfig,
    hook: &ServiceHook,
    phase: HookPhase,
    workspace_root: &Path,
) -> Result<()> {
    match &hook.run {
        HookRun::Exec { argv } => run_exec_hook(service, argv, phase),
        HookRun::Script { path } => run_script_hook(path, hook.timeout_sec, workspace_root),
    }
}

fn run_exec_hook(service: &ServiceConfig, argv: &[String], phase: HookPhase) -> Result<()> {
    if phase == HookPhase::PostDown {
        anyhow::bail!("`run.type=exec` is not supported for post_down hooks");
    }
    if argv.is_empty() {
        anyhow::bail!("`run.argv` must include at least one command token");
    }
    crate::docker::exec_command_to_stderr(service, argv)
}

pub(super) const fn hook_phase_label(phase: HookPhase) -> &'static str {
    match phase {
        HookPhase::PostUp => "post_up",
        HookPhase::PreDown => "pre_down",
        HookPhase::PostDown => "post_down",
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;
    use std::path::Path;
    use std::time::{SystemTime, UNIX_EPOCH};

    use crate::config::{
        Driver, HookOnError, HookPhase, HookRun, Kind, ServiceConfig, ServiceHook,
    };

    use super::{execute_hook, run_exec_hook};
    use crate::docker::with_dry_run_lock;

    fn service() -> ServiceConfig {
        ServiceConfig {
            name: "app".to_owned(),
            kind: Kind::App,
            driver: Driver::Frankenphp,
            image: "dunglas/frankenphp:php8.5".to_owned(),
            host: "127.0.0.1".to_owned(),
            port: 9000,
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
            routes: None,
            container_port: None,
            smtp_port: None,
            volumes: None,
            env: None,
            command: None,
            depends_on: None,
            seed_file: None,
            hook: Vec::new(),
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
            container_name: Some("app".to_owned()),
            resolved_container_name: Some("app".to_owned()),
            runtime: Default::default(),
        }
    }

    #[test]
    fn execute_hook_skips_exec_in_post_down() {
        let result = execute_hook(
            &service(),
            &ServiceHook {
                name: "exec".to_owned(),
                phase: HookPhase::PostDown,
                run: HookRun::Exec {
                    argv: vec!["echo".to_owned()],
                },
                on_error: HookOnError::Fail,
                timeout_sec: None,
            },
            HookPhase::PostDown,
            Path::new("/tmp"),
        );

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not supported"));
    }

    #[test]
    fn execute_hook_rejects_empty_exec_argv() {
        let result = execute_hook(
            &service(),
            &ServiceHook {
                name: "exec".to_owned(),
                phase: HookPhase::PostUp,
                run: HookRun::Exec { argv: Vec::new() },
                on_error: HookOnError::Fail,
                timeout_sec: None,
            },
            HookPhase::PostUp,
            Path::new("/tmp"),
        );

        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("must include at least one")
        );
    }

    #[test]
    fn execute_hook_runs_exec_in_dry_run() {
        with_dry_run_lock(|| {
            execute_hook(
                &service(),
                &ServiceHook {
                    name: "exec".to_owned(),
                    phase: HookPhase::PostUp,
                    run: HookRun::Exec {
                        argv: vec!["echo".to_owned(), "ok".to_owned()],
                    },
                    on_error: HookOnError::Fail,
                    timeout_sec: None,
                },
                HookPhase::PostUp,
                Path::new("/tmp"),
            )
            .expect("exec hook");
        });
    }

    #[test]
    fn execute_hook_runs_script() {
        let bin_dir = crate::test_support::temp_root().join(format!(
            "berth-hook-script-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        std::fs::create_dir_all(&bin_dir).expect("fake script dir");
        let script = bin_dir.join("hook.sh");
        let mut file = std::fs::File::create(&script).expect("script");
        file.write_all(b"#!/bin/sh\nexit 0\n").expect("write");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&script).expect("metadata").permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&script, perms).expect("chmod");
        }

        execute_hook(
            &service(),
            &ServiceHook {
                name: "script".to_owned(),
                phase: HookPhase::PostUp,
                run: HookRun::Script {
                    path: script.to_string_lossy().into_owned(),
                },
                on_error: HookOnError::Fail,
                timeout_sec: None,
            },
            HookPhase::PostUp,
            &bin_dir,
        )
        .expect("script hook");
        std::fs::remove_dir_all(&bin_dir).ok();
    }

    #[test]
    fn run_exec_hook_executes_docker_command_when_not_post_down() {
        let service = service();
        with_dry_run_lock(|| {
            run_exec_hook(
                &service,
                &["echo".to_owned(), "ok".to_owned()],
                HookPhase::PreDown,
            )
            .expect("run exec");
        });
    }
}
