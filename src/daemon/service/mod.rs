//! User-service definitions for login-time daemon watch startup.

mod launchd;
mod systemd;

use anyhow::{Context, Result, bail};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[cfg(test)]
use std::cell::RefCell;

use super::daemon_binary;

const LAUNCHD_LABEL: &str = "dev.berth.daemon.watch";
const SYSTEMD_UNIT_NAME: &str = "berth-daemon-watch.service";

#[cfg(test)]
thread_local! {
    static TEST_SERVICE_HOME: RefCell<Option<String>> = const { RefCell::new(None) };
    static TEST_SERVICE_BINARY: RefCell<Option<String>> = const { RefCell::new(None) };
    static TEST_SERVICE_MANAGER: RefCell<Option<ServiceManager>> = const { RefCell::new(None) };
    static TEST_SERVICE_COMMANDS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ServiceManager {
    Launchd,
    SystemdUser,
}

#[derive(Debug, Clone)]
pub(crate) struct DaemonServiceInstallOptions {
    pub(crate) watch_dirs: Vec<PathBuf>,
    pub(crate) exclude_dirs: Vec<PathBuf>,
    pub(crate) max_projects: Option<usize>,
    pub(crate) interval_secs: u64,
}

#[derive(Debug, Clone)]
pub(crate) struct DaemonServiceDefinition {
    pub(crate) manager: ServiceManager,
    pub(crate) label: String,
    pub(crate) path: PathBuf,
    pub(crate) contents: String,
}

#[derive(Debug, Clone)]
pub(crate) struct DaemonServiceStatus {
    pub(crate) manager: ServiceManager,
    pub(crate) label: String,
    pub(crate) path: PathBuf,
    pub(crate) installed: bool,
}

pub(crate) fn install_service(
    options: &DaemonServiceInstallOptions,
) -> Result<DaemonServiceStatus> {
    let definition = service_definition(options)?;
    if let Some(parent) = definition.path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    fs::write(&definition.path, &definition.contents)
        .with_context(|| format!("failed to write {}", definition.path.display()))?;

    match definition.manager {
        ServiceManager::Launchd => install_launchd(&definition)?,
        ServiceManager::SystemdUser => install_systemd(&definition)?,
    }

    Ok(DaemonServiceStatus {
        manager: definition.manager,
        label: definition.label,
        path: definition.path,
        installed: true,
    })
}

pub(crate) fn uninstall_service() -> Result<DaemonServiceStatus> {
    let manager = service_manager()?;
    let definition = service_definition(&DaemonServiceInstallOptions {
        watch_dirs: Vec::new(),
        exclude_dirs: Vec::new(),
        max_projects: None,
        interval_secs: 30,
    })?;
    let installed = definition.path.exists();

    match manager {
        ServiceManager::Launchd => uninstall_launchd(&definition.path)?,
        ServiceManager::SystemdUser => uninstall_systemd(&definition.path)?,
    }

    if definition.path.exists() {
        fs::remove_file(&definition.path)
            .with_context(|| format!("failed to remove {}", definition.path.display()))?;
    }

    Ok(DaemonServiceStatus {
        manager,
        label: definition.label,
        path: definition.path,
        installed,
    })
}

pub(crate) fn service_status() -> Result<DaemonServiceStatus> {
    let definition = service_definition(&DaemonServiceInstallOptions {
        watch_dirs: Vec::new(),
        exclude_dirs: Vec::new(),
        max_projects: None,
        interval_secs: 30,
    })?;
    Ok(DaemonServiceStatus {
        manager: definition.manager,
        label: definition.label,
        installed: definition.path.exists(),
        path: definition.path,
    })
}

pub(crate) fn print_service(
    options: &DaemonServiceInstallOptions,
) -> Result<DaemonServiceDefinition> {
    service_definition(options)
}

fn service_definition(options: &DaemonServiceInstallOptions) -> Result<DaemonServiceDefinition> {
    let manager = service_manager()?;
    match manager {
        ServiceManager::Launchd => launchd::definition(
            &service_context(options)?,
            LAUNCHD_LABEL,
            &launchd_plist_path()?,
        ),
        ServiceManager::SystemdUser => systemd::definition(
            &service_context(options)?,
            SYSTEMD_UNIT_NAME,
            &systemd_unit_path()?,
        ),
    }
}

fn service_context(options: &DaemonServiceInstallOptions) -> Result<ServiceContext> {
    let binary = daemon_service_binary()?;
    let args = watch_command_args(options);
    Ok(ServiceContext {
        binary,
        args,
        stdout_path: daemon_state_home()?.join("watch-service.stdout.log"),
        stderr_path: daemon_state_home()?.join("watch-service.stderr.log"),
    })
}

fn watch_command_args(options: &DaemonServiceInstallOptions) -> Vec<String> {
    let mut args = vec![
        "daemon".to_owned(),
        "watch".to_owned(),
        "--interval".to_owned(),
        options.interval_secs.max(1).to_string(),
    ];
    for dir in &options.watch_dirs {
        args.push("--dir".to_owned());
        args.push(dir.to_string_lossy().into_owned());
    }
    for dir in &options.exclude_dirs {
        args.push("--exclude-dir".to_owned());
        args.push(dir.to_string_lossy().into_owned());
    }
    if let Some(limit) = options.max_projects {
        args.push("--max-projects".to_owned());
        args.push(limit.to_string());
    }
    args
}

fn install_launchd(definition: &DaemonServiceDefinition) -> Result<()> {
    let domain = launchd_domain()?;
    run_command(
        "launchctl",
        &[
            "bootout".to_owned(),
            domain.clone(),
            definition.path.to_string_lossy().into_owned(),
        ],
        true,
    )?;
    run_command(
        "launchctl",
        &[
            "bootstrap".to_owned(),
            domain,
            definition.path.to_string_lossy().into_owned(),
        ],
        false,
    )
}

fn uninstall_launchd(path: &Path) -> Result<()> {
    let domain = launchd_domain()?;
    run_command(
        "launchctl",
        &[
            "bootout".to_owned(),
            domain,
            path.to_string_lossy().into_owned(),
        ],
        true,
    )
}

fn install_systemd(definition: &DaemonServiceDefinition) -> Result<()> {
    run_command(
        "systemctl",
        &["--user".to_owned(), "daemon-reload".to_owned()],
        false,
    )?;
    run_command(
        "systemctl",
        &[
            "--user".to_owned(),
            "enable".to_owned(),
            "--now".to_owned(),
            definition.label.clone(),
        ],
        false,
    )
}

fn uninstall_systemd(path: &Path) -> Result<()> {
    let label = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or(SYSTEMD_UNIT_NAME)
        .to_owned();
    run_command(
        "systemctl",
        &[
            "--user".to_owned(),
            "disable".to_owned(),
            "--now".to_owned(),
            label,
        ],
        true,
    )?;
    run_command(
        "systemctl",
        &["--user".to_owned(), "daemon-reload".to_owned()],
        true,
    )
}

fn run_command(program: &str, args: &[String], allow_failure: bool) -> Result<()> {
    #[cfg(test)]
    if test_mode_enabled() {
        TEST_SERVICE_COMMANDS.with(|commands| {
            let mut commands = commands.borrow_mut();
            let rendered = std::iter::once(program.to_owned())
                .chain(args.iter().cloned())
                .collect::<Vec<_>>()
                .join(" ");
            commands.push(rendered);
        });
        return Ok(());
    }

    let status = Command::new(program)
        .args(args)
        .status()
        .with_context(|| format!("failed to run {}", program))?;
    if status.success() || allow_failure {
        return Ok(());
    }

    bail!("{} exited with {}", program, status);
}

fn launchd_domain() -> Result<String> {
    let output = Command::new("id")
        .arg("-u")
        .output()
        .context("failed to resolve current uid")?;
    if !output.status.success() {
        bail!("failed to resolve current uid");
    }
    let uid = String::from_utf8(output.stdout).context("uid was not valid UTF-8")?;
    Ok(format!("gui/{}", uid.trim()))
}

fn service_manager() -> Result<ServiceManager> {
    #[cfg(test)]
    if let Some(manager) = TEST_SERVICE_MANAGER.with(|value| *value.borrow()) {
        return Ok(manager);
    }

    if cfg!(target_os = "macos") {
        return Ok(ServiceManager::Launchd);
    }
    if cfg!(target_os = "linux") {
        return Ok(ServiceManager::SystemdUser);
    }

    bail!("daemon services are only supported on macOS and Linux");
}

fn daemon_service_binary() -> Result<String> {
    #[cfg(test)]
    if let Some(binary) = TEST_SERVICE_BINARY.with(|value| value.borrow().clone()) {
        return Ok(binary);
    }

    daemon_binary()
}

fn home_dir() -> Result<PathBuf> {
    #[cfg(test)]
    if let Some(home) = TEST_SERVICE_HOME.with(|value| value.borrow().clone()) {
        return Ok(PathBuf::from(home));
    }

    let home = std::env::var("HOME").context("HOME is not set")?;
    Ok(PathBuf::from(home))
}

fn daemon_state_home() -> Result<PathBuf> {
    Ok(home_dir()?.join(".config/berth/daemon"))
}

fn launchd_plist_path() -> Result<PathBuf> {
    Ok(home_dir()?
        .join("Library")
        .join("LaunchAgents")
        .join(format!("{LAUNCHD_LABEL}.plist")))
}

fn systemd_unit_path() -> Result<PathBuf> {
    Ok(home_dir()?
        .join(".config")
        .join("systemd")
        .join("user")
        .join(SYSTEMD_UNIT_NAME))
}

#[cfg(test)]
fn test_mode_enabled() -> bool {
    TEST_SERVICE_HOME.with(|value| value.borrow().is_some())
        || TEST_SERVICE_BINARY.with(|value| value.borrow().is_some())
        || TEST_SERVICE_MANAGER.with(|value| value.borrow().is_some())
}

#[cfg(test)]
pub(crate) fn set_test_service_home(home: &str) {
    TEST_SERVICE_HOME.with(|value| *value.borrow_mut() = Some(home.to_owned()));
}

#[cfg(test)]
pub(crate) fn clear_test_service_home() {
    TEST_SERVICE_HOME.with(|value| *value.borrow_mut() = None);
}

#[cfg(test)]
pub(crate) fn set_test_service_binary(path: &str) {
    TEST_SERVICE_BINARY.with(|value| *value.borrow_mut() = Some(path.to_owned()));
}

#[cfg(test)]
pub(crate) fn clear_test_service_binary() {
    TEST_SERVICE_BINARY.with(|value| *value.borrow_mut() = None);
}

#[cfg(test)]
pub(crate) fn set_test_service_manager(manager: ServiceManager) {
    TEST_SERVICE_MANAGER.with(|value| *value.borrow_mut() = Some(manager));
}

#[cfg(test)]
pub(crate) fn clear_test_service_manager() {
    TEST_SERVICE_MANAGER.with(|value| *value.borrow_mut() = None);
}

#[cfg(test)]
pub(crate) fn clear_test_service_commands() {
    TEST_SERVICE_COMMANDS.with(|value| value.borrow_mut().clear());
}

#[cfg(test)]
pub(crate) fn take_test_service_commands() -> Vec<String> {
    TEST_SERVICE_COMMANDS.with(|value| std::mem::take(&mut *value.borrow_mut()))
}

struct ServiceContext {
    binary: String,
    args: Vec<String>,
    stdout_path: PathBuf,
    stderr_path: PathBuf,
}

#[cfg(test)]
mod tests {
    use super::{
        DaemonServiceInstallOptions, ServiceManager, clear_test_service_binary,
        clear_test_service_commands, clear_test_service_home, clear_test_service_manager,
        install_service, print_service, service_status, set_test_service_binary,
        set_test_service_home, set_test_service_manager, take_test_service_commands,
        uninstall_service,
    };
    use std::fs;

    fn temp_home(name: &str) -> std::path::PathBuf {
        let root = crate::test_support::temp_root().join(format!(
            "berth-daemon-service-{name}-{}",
            crate::test_support::unique_stamp()
        ));
        drop(fs::remove_dir_all(&root));
        fs::create_dir_all(&root).expect("create temp root");
        root
    }

    fn service_options() -> DaemonServiceInstallOptions {
        DaemonServiceInstallOptions {
            watch_dirs: vec![std::path::PathBuf::from("/tmp/projects")],
            exclude_dirs: vec![std::path::PathBuf::from("/tmp/archive")],
            max_projects: Some(5),
            interval_secs: 45,
        }
    }

    #[test]
    fn install_service_writes_launchd_plist_and_records_commands() {
        let home = temp_home("launchd");
        set_test_service_home(home.to_str().expect("home path"));
        set_test_service_binary("/tmp/berth");
        set_test_service_manager(ServiceManager::Launchd);
        clear_test_service_commands();

        let status = install_service(&service_options()).expect("install launchd");
        let plist = fs::read_to_string(&status.path).expect("read plist");

        assert!(status.installed);
        assert!(plist.contains("/tmp/berth"));
        assert!(plist.contains("<string>--dir</string>"));
        assert!(plist.contains("<string>/tmp/projects</string>"));
        assert!(plist.contains("<string>--exclude-dir</string>"));
        assert!(plist.contains("<string>/tmp/archive</string>"));
        assert!(plist.contains("<string>--max-projects</string>"));
        assert!(plist.contains("<string>5</string>"));

        let commands = take_test_service_commands();
        assert_eq!(commands.len(), 2);
        assert!(commands[0].contains("launchctl bootout"));
        assert!(commands[1].contains("launchctl bootstrap"));

        clear_test_service_binary();
        clear_test_service_home();
        clear_test_service_manager();
    }

    #[test]
    fn install_service_writes_systemd_unit_and_records_commands() {
        let home = temp_home("systemd");
        set_test_service_home(home.to_str().expect("home path"));
        set_test_service_binary("/tmp/berth");
        set_test_service_manager(ServiceManager::SystemdUser);
        clear_test_service_commands();

        let status = install_service(&service_options()).expect("install systemd");
        let unit = fs::read_to_string(&status.path).expect("read unit");

        assert!(status.installed);
        assert!(unit.contains("ExecStart="));
        assert!(unit.contains("/tmp/berth"));
        assert!(unit.contains("--interval"));
        assert!(unit.contains("--exclude-dir"));
        assert!(unit.contains("--max-projects"));

        let commands = take_test_service_commands();
        assert_eq!(commands.len(), 2);
        assert_eq!(commands[0], "systemctl --user daemon-reload");
        assert_eq!(
            commands[1],
            "systemctl --user enable --now berth-daemon-watch.service"
        );

        clear_test_service_binary();
        clear_test_service_home();
        clear_test_service_manager();
    }

    #[test]
    fn uninstall_service_removes_installed_definition() {
        let home = temp_home("uninstall");
        set_test_service_home(home.to_str().expect("home path"));
        set_test_service_binary("/tmp/berth");
        set_test_service_manager(ServiceManager::SystemdUser);
        clear_test_service_commands();

        let installed = install_service(&service_options()).expect("install");
        assert!(installed.path.exists());
        let removed = uninstall_service().expect("uninstall");

        assert!(removed.installed);
        assert!(!removed.path.exists());

        let commands = take_test_service_commands();
        assert!(commands.iter().any(|command| {
            command == "systemctl --user disable --now berth-daemon-watch.service"
        }));

        clear_test_service_binary();
        clear_test_service_home();
        clear_test_service_manager();
    }

    #[test]
    fn print_and_status_report_service_definition() {
        let home = temp_home("status");
        set_test_service_home(home.to_str().expect("home path"));
        set_test_service_binary("/tmp/berth");
        set_test_service_manager(ServiceManager::Launchd);

        let printed = print_service(&service_options()).expect("print");
        assert_eq!(printed.label, "dev.berth.daemon.watch");
        assert!(printed.contents.contains("/tmp/berth"));

        let status_before = service_status().expect("status before install");
        assert!(!status_before.installed);

        install_service(&service_options()).expect("install");
        let status_after = service_status().expect("status after install");
        assert!(status_after.installed);

        clear_test_service_binary();
        clear_test_service_home();
        clear_test_service_manager();
    }
}
