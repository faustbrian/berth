//! cli handlers daemon cmd module.
//!
//! Contains pre-config daemon command routing used by Berth command workflows.

mod service;

use crate::cli::args::{
    DaemonArgs, DaemonCommands, DaemonLogsArgs, DaemonRunArgs, DaemonStartArgs, DaemonStatusArgs,
    DaemonStopArgs, DaemonWatchArgs,
};
use crate::config;
use crate::daemon::{self, DaemonSession, DiscoveryOptions, DiscoveryReport};
use crate::output::{self, LogLevel, Persistence};
use anyhow::Result;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

pub(crate) fn handle_daemon(args: &DaemonArgs) -> Result<()> {
    match &args.command {
        DaemonCommands::Start(start) => handle_daemon_start(start),
        DaemonCommands::Watch(watch) => handle_daemon_watch(watch),
        DaemonCommands::Service(service_args) => service::handle_daemon_service(service_args),
        DaemonCommands::Status(status) => handle_daemon_status(status),
        DaemonCommands::Stop(stop) => handle_daemon_stop(stop),
        DaemonCommands::Logs(logs) => handle_daemon_logs(logs),
        DaemonCommands::Run(run) => handle_daemon_run(run),
    }
}

fn handle_daemon_start(args: &DaemonStartArgs) -> Result<()> {
    let project_root = resolve_daemon_project_root(&args.path)?;
    start_daemon_for_project_root(&project_root, true)?;
    Ok(())
}

fn handle_daemon_watch(args: &DaemonWatchArgs) -> Result<()> {
    loop {
        let report = daemon::discover_projects(&watch_discovery_options(args))?;
        log_discovery_report(&report);
        for project_root in &report.projects {
            start_daemon_for_project_root(project_root, false)?;
        }

        if args.once {
            return Ok(());
        }

        thread::sleep(Duration::from_secs(args.interval.max(1)));
    }
}

fn start_daemon_for_project_root(project_root: &Path, log_existing: bool) -> Result<bool> {
    if let Some(existing) = daemon::load_session(&project_root)?
        && daemon::pid_is_running(existing.pid)
    {
        if log_existing {
            output::event(
                "daemon",
                LogLevel::Info,
                &format!(
                    "Daemon already running for {} (pid {})",
                    project_root.display(),
                    existing.pid
                ),
                Persistence::Persistent,
            );
        }
        return Ok(false);
    }

    let log_path = daemon::daemon_log_path(&project_root)?;
    let pid = daemon::spawn_detached(&project_root, &log_path)?;
    daemon::save_session(
        &project_root,
        &DaemonSession {
            project_root: project_root.to_string_lossy().into_owned(),
            pid,
            log_path: log_path.to_string_lossy().into_owned(),
            started_at_unix: daemon::now_unix(),
        },
    )?;
    output::event(
        "daemon",
        LogLevel::Success,
        &format!(
            "Started daemon for {} (pid {})",
            project_root.display(),
            pid
        ),
        Persistence::Persistent,
    );
    Ok(true)
}

fn log_discovery_report(report: &DiscoveryReport) {
    for invalid in &report.invalid_configs {
        output::event(
            "daemon",
            LogLevel::Error,
            &format!(
                "Skipping invalid Berth project config at {}",
                invalid.display()
            ),
            Persistence::Persistent,
        );
    }

    for duplicate in &report.duplicate_projects {
        output::event(
            "daemon",
            LogLevel::Info,
            &format!(
                "Skipping duplicate discovered Berth project at {}",
                duplicate.display()
            ),
            Persistence::Persistent,
        );
    }

    for limited in &report.limited_projects {
        output::event(
            "daemon",
            LogLevel::Info,
            &format!(
                "Skipping discovered Berth project at {} because the watch project limit was reached",
                limited.display()
            ),
            Persistence::Persistent,
        );
    }
}

fn watch_discovery_options(args: &DaemonWatchArgs) -> DiscoveryOptions {
    DiscoveryOptions {
        watch_dirs: args.dir.clone(),
        exclude_dirs: args.policy.exclude_dir.clone(),
        max_projects: args.policy.max_projects,
    }
}

fn handle_daemon_status(args: &DaemonStatusArgs) -> Result<()> {
    let project_root = resolve_daemon_project_root(&args.path)?;
    let Some(session) = daemon::load_session(&project_root)? else {
        output::event(
            "daemon",
            LogLevel::Info,
            &format!("No daemon session recorded for {}", project_root.display()),
            Persistence::Persistent,
        );
        return Ok(());
    };
    let message = if daemon::pid_is_running(session.pid) {
        format!(
            "Daemon running for {} (pid {})",
            project_root.display(),
            session.pid
        )
    } else {
        format!(
            "Daemon session for {} is stale (pid {})",
            project_root.display(),
            session.pid
        )
    };
    output::event("daemon", LogLevel::Info, &message, Persistence::Persistent);
    Ok(())
}

fn handle_daemon_stop(args: &DaemonStopArgs) -> Result<()> {
    let project_root = resolve_daemon_project_root(&args.path)?;
    let Some(session) = daemon::load_session(&project_root)? else {
        output::event(
            "daemon",
            LogLevel::Info,
            &format!("No daemon session running for {}", project_root.display()),
            Persistence::Persistent,
        );
        return Ok(());
    };
    daemon::stop_pid(session.pid)?;
    daemon::clear_session(&project_root)?;
    output::event(
        "daemon",
        LogLevel::Success,
        &format!(
            "Stopped daemon for {} (pid {})",
            project_root.display(),
            session.pid
        ),
        Persistence::Persistent,
    );
    Ok(())
}

fn handle_daemon_logs(args: &DaemonLogsArgs) -> Result<()> {
    let project_root = resolve_daemon_project_root(&args.path)?;
    let Some(session) = daemon::load_session(&project_root)? else {
        output::event(
            "daemon",
            LogLevel::Info,
            &format!("No daemon logs found for {}", project_root.display()),
            Persistence::Persistent,
        );
        return Ok(());
    };
    let content = std::fs::read(&session.log_path)?;
    std::io::stdout().write_all(&content)?;
    std::io::stdout().flush()?;
    Ok(())
}

fn handle_daemon_run(args: &DaemonRunArgs) -> Result<()> {
    let project_root = resolve_daemon_project_root(&args.path)?;
    daemon::run_supervisor(&project_root)
}

fn resolve_daemon_project_root(path: &Path) -> Result<PathBuf> {
    config::project_root_with(config::ProjectRootPathOptions::new(None, Some(path)))
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use crate::cli::args::{
        DaemonArgs, DaemonCommands, DaemonStartArgs, DaemonStatusArgs, DaemonStopArgs,
        DaemonWatchArgs, DaemonWatchPolicyArgs,
    };

    fn temp_project_root(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "berth-daemon-handler-{name}-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock")
                .as_nanos()
        ));
        drop(fs::remove_dir_all(&root));
        fs::create_dir_all(&root).expect("create project root");
        fs::write(
            root.join(".berth.toml"),
            "schema_version = 1\nproject_type = \"project\"\nservice = []\nswarm = []\n",
        )
        .expect("write berth config");
        root
    }

    fn temp_home(name: &str) -> PathBuf {
        let home = std::env::temp_dir().join(format!(
            "berth-daemon-home-{name}-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock")
                .as_nanos()
        ));
        drop(fs::remove_dir_all(&home));
        fs::create_dir_all(&home).expect("create temporary home");
        home
    }

    fn mock_daemon_binary(home: &PathBuf) -> String {
        let script = home.join("berth-daemon-mock");
        fs::write(&script, "#!/usr/bin/env sh\nexec sleep 60\n").expect("write mock daemon");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&script)
                .expect("script metadata")
                .permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&script, perms).expect("chmod mock daemon");
        }
        script.to_string_lossy().to_string()
    }

    #[test]
    fn daemon_start_persists_session_and_stop_clears_it() {
        let project_root = temp_project_root("start-stop");
        let home = temp_home("start-stop");
        let binary = mock_daemon_binary(&home);

        crate::daemon::set_test_daemon_home(home.to_str().expect("home path"));
        crate::daemon::set_test_daemon_binary(&binary);

        super::handle_daemon(&DaemonArgs {
            command: DaemonCommands::Start(DaemonStartArgs {
                path: project_root.clone(),
            }),
        })
        .expect("start daemon");

        let session = crate::daemon::load_session(&project_root)
            .expect("load daemon session")
            .expect("daemon session should exist");
        assert!(crate::daemon::pid_is_running(session.pid));

        super::handle_daemon(&DaemonArgs {
            command: DaemonCommands::Status(DaemonStatusArgs {
                path: project_root.clone(),
            }),
        })
        .expect("status daemon");

        super::handle_daemon(&DaemonArgs {
            command: DaemonCommands::Stop(DaemonStopArgs {
                path: project_root.clone(),
            }),
        })
        .expect("stop daemon");

        assert!(
            crate::daemon::load_session(&project_root)
                .expect("load daemon session after stop")
                .is_none()
        );

        crate::daemon::clear_test_daemon_binary();
        crate::daemon::clear_test_daemon_home();
    }

    #[test]
    fn daemon_watch_once_discovers_projects_and_starts_missing_daemons() {
        let watch_root = temp_home("watch-root");
        let project_root = watch_root.join("project-a");
        fs::create_dir_all(&project_root).expect("create project root");
        fs::write(
            project_root.join(".berth.toml"),
            "schema_version = 1\nproject_type = \"project\"\nservice = []\nswarm = []\n",
        )
        .expect("write berth config");

        let home = temp_home("watch-home");
        let binary = mock_daemon_binary(&home);
        crate::daemon::set_test_daemon_home(home.to_str().expect("home path"));
        crate::daemon::set_test_daemon_binary(&binary);

        super::handle_daemon(&DaemonArgs {
            command: DaemonCommands::Watch(DaemonWatchArgs {
                dir: vec![watch_root.clone()],
                policy: DaemonWatchPolicyArgs {
                    exclude_dir: Vec::new(),
                    max_projects: None,
                },
                once: true,
                interval: 1,
            }),
        })
        .expect("watch once");

        let session = crate::daemon::load_session(&project_root)
            .expect("load watch session")
            .expect("watch should create daemon session");
        assert!(crate::daemon::pid_is_running(session.pid));

        super::handle_daemon(&DaemonArgs {
            command: DaemonCommands::Stop(DaemonStopArgs {
                path: project_root.clone(),
            }),
        })
        .expect("stop watched daemon");

        crate::daemon::clear_test_daemon_binary();
        crate::daemon::clear_test_daemon_home();
    }
}
