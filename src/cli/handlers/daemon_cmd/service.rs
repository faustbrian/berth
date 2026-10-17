//! `berth daemon service` command handlers.

use crate::cli::args::{
    DaemonServiceArgs, DaemonServiceCommands, DaemonServiceInstallArgs, DaemonServicePrintArgs,
    DaemonWatchPolicyArgs,
};
use crate::daemon::{self, DaemonServiceInstallOptions, ServiceManager};
use crate::output::{self, LogLevel, Persistence};
use anyhow::Result;
use std::io::Write;

pub(super) fn handle_daemon_service(args: &DaemonServiceArgs) -> Result<()> {
    match &args.command {
        DaemonServiceCommands::Install(install) => handle_install(install),
        DaemonServiceCommands::Status => handle_status(),
        DaemonServiceCommands::Print(print) => handle_print(print),
        DaemonServiceCommands::Uninstall => handle_uninstall(),
    }
}

fn handle_install(args: &DaemonServiceInstallArgs) -> Result<()> {
    let status = daemon::install_service(&install_options(
        args.dir.clone(),
        &args.policy,
        args.interval,
    ))?;
    output::event(
        "daemon",
        LogLevel::Success,
        &format!(
            "Installed {} daemon watch service {} at {}",
            manager_name(status.manager),
            status.label,
            status.path.display()
        ),
        Persistence::Persistent,
    );
    Ok(())
}

fn handle_status() -> Result<()> {
    let status = daemon::service_status()?;
    let message = if status.installed {
        format!(
            "{} daemon watch service {} installed at {}",
            manager_name(status.manager),
            status.label,
            status.path.display()
        )
    } else {
        format!(
            "No {} daemon watch service {} installed at {}",
            manager_name(status.manager),
            status.label,
            status.path.display()
        )
    };
    output::event("daemon", LogLevel::Info, &message, Persistence::Persistent);
    Ok(())
}

fn handle_print(args: &DaemonServicePrintArgs) -> Result<()> {
    let definition = daemon::print_service(&install_options(
        args.dir.clone(),
        &args.policy,
        args.interval,
    ))?;
    std::io::stdout().write_all(definition.contents.as_bytes())?;
    std::io::stdout().flush()?;
    Ok(())
}

fn handle_uninstall() -> Result<()> {
    let status = daemon::uninstall_service()?;
    let message = if status.installed {
        format!(
            "Removed {} daemon watch service {} from {}",
            manager_name(status.manager),
            status.label,
            status.path.display()
        )
    } else {
        format!(
            "No {} daemon watch service {} was installed at {}",
            manager_name(status.manager),
            status.label,
            status.path.display()
        )
    };
    output::event(
        "daemon",
        LogLevel::Success,
        &message,
        Persistence::Persistent,
    );
    Ok(())
}

fn install_options(
    dirs: Vec<std::path::PathBuf>,
    policy: &DaemonWatchPolicyArgs,
    interval: u64,
) -> DaemonServiceInstallOptions {
    DaemonServiceInstallOptions {
        watch_dirs: dirs,
        exclude_dirs: policy.exclude_dir.clone(),
        max_projects: policy.max_projects,
        interval_secs: interval,
    }
}

fn manager_name(manager: ServiceManager) -> &'static str {
    match manager {
        ServiceManager::Launchd => "launchd",
        ServiceManager::SystemdUser => "systemd --user",
    }
}
