//! The `berth` commands an error tells the user to run next.
//!
//! A hint that names a follow-up command has to reach the same project,
//! instance and runtime environment as the command that failed: `--instance`
//! lives only in one process, so a bare `berth down --purge` from an
//! `--instance agent-2` run purges the checkout's default instance. Every
//! such hint is built here, from the invocation's own flags or from the
//! service the error is about.

use std::path::{Path, PathBuf};

use crate::config::ServiceConfig;

/// The context flags of the running invocation, as the user passed them.
#[derive(Clone, Debug, Default)]
pub(crate) struct Invocation {
    pub(crate) config: Option<PathBuf>,
    pub(crate) project_root: Option<PathBuf>,
    pub(crate) instance: Option<String>,
    pub(crate) env: Option<String>,
    /// `--engine` (or `BERTH_ENGINE`). A project command reads the project
    /// file's `container_engine` again; installation-wide ones do not (see
    /// [`global`]).
    pub(crate) engine: Option<crate::config::ContainerEngine>,
    /// Whether this run is a `--dry-run` preview.
    pub(crate) dry_run: bool,
    /// Whether this run enforces the image lockfile (`--repro`).
    pub(crate) repro: bool,
    /// Whether this run is installation-wide (`ls`, `gc`) and so has no
    /// project for a project command to load.
    pub(crate) global: bool,
    /// The flag that limits this run's readiness waits.
    pub(crate) limit: Limit,
}

/// How a command limits its readiness waits; each names it differently.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Limit {
    /// `--wait-timeout`, optional (`up`, `start`, `recreate`, `restart`, …).
    #[default]
    WaitTimeout,
    /// `setup --timeout`, optional.
    SetupTimeout,
    /// `health --timeout`, always set (30s by default).
    HealthTimeout,
    /// No flag: `doctor --fix` waits a fixed time after a repair.
    Fixed,
}

#[cfg(not(test))]
static INVOCATION: std::sync::OnceLock<Invocation> = std::sync::OnceLock::new();

#[cfg(test)]
thread_local! {
    static INVOCATION: std::cell::RefCell<Invocation> =
        std::cell::RefCell::new(Invocation::default());
}

/// Records the invocation's context flags once, at dispatch.
pub(crate) fn record(invocation: Invocation) {
    #[cfg(not(test))]
    drop(INVOCATION.set(invocation));
    #[cfg(test)]
    INVOCATION.with(|current| *current.borrow_mut() = invocation);
}

fn invocation() -> Invocation {
    #[cfg(not(test))]
    return INVOCATION.get().cloned().unwrap_or_default();
    #[cfg(test)]
    INVOCATION.with(|current| current.borrow().clone())
}

/// `berth {args}` with the running invocation's own context flags: for
/// "run this again" hints, which must target what the failing command did.
#[must_use]
pub(crate) fn command(args: &str) -> String {
    let invocation = invocation();
    let mut flags = engine_flag(&invocation);
    if let Some(config) = &invocation.config {
        flags.push(format!("--config {}", quote_path(&absolute(config))));
    }
    if let Some(root) = &invocation.project_root {
        flags.push(format!("--project-root {}", quote_path(&absolute(root))));
    }
    if let Some(instance) = &invocation.instance {
        flags.push(format!("--instance {}", quote(instance)));
    }
    if let Some(env) = &invocation.env {
        flags.push(format!("--env {}", quote(env)));
    }
    flags.extend(mode_flags(&invocation));
    render(&flags, args)
}

/// How to give this run's readiness waits longer, for a wait that hit its
/// limit.
#[must_use]
pub(crate) fn raise_limit() -> String {
    match invocation().limit {
        Limit::WaitTimeout => {
            "raise `--wait-timeout`, or leave it out to wait while it runs".to_owned()
        }
        Limit::SetupTimeout => {
            "raise `--timeout`, or leave it out to wait while it runs".to_owned()
        }
        Limit::HealthTimeout => "raise `--timeout` (30s by default)".to_owned(),
        Limit::Fixed => format!(
            "the repair waits a fixed time, so start it with {} and let it finish",
            command("up")
        ),
    }
}

/// `berth {args}` for an installation-wide command (`gc`, `ls`). Those
/// never read a project file and default to docker, so the engine this run
/// uses (a project's `container_engine: podman` too) is named whenever it is
/// not that default.
#[must_use]
pub(crate) fn global(args: &str) -> String {
    let invocation = invocation();
    let active = crate::docker::container_engine();
    let mut flags = engine_flag(&invocation);
    if flags.is_empty() && active == crate::config::ContainerEngine::Podman {
        flags.push("--engine podman".to_owned());
    }
    render(&flags, args)
}

/// Where `berth doctor` checks the setup: in this run's project, or, from an
/// installation-wide command, in any project (doctor needs one to load).
#[must_use]
pub(crate) fn doctor() -> String {
    if invocation().global {
        format!("{} (run in a project) checks the setup", global("doctor"))
    } else {
        format!("{} checks the setup", command("doctor"))
    }
}

/// `berth {args}` aimed at `service`'s own project, instance and runtime
/// environment, so it works from anywhere (a swarm source's service, too).
#[must_use]
pub(crate) fn for_service(service: &ServiceConfig, args: &str) -> String {
    let invocation = invocation();
    let mut flags = engine_flag(&invocation);
    if let Some(root) = &service.runtime.root {
        flags.push(project_flag(&invocation, root));
    }
    // The instance stays: a test runtime lives in its instance's namespace.
    if let Some(instance) = &service.runtime.instance {
        flags.push(format!("--instance {}", quote(instance)));
    }
    // A test runtime's env (`testing-pool-…`) is generated per run and its
    // containers are reset when it fails, so no command can reach them
    // again: the way back is the test run itself (`doctor` only needs the
    // instance, not the lease).
    let lease = service
        .runtime
        .env
        .as_deref()
        .filter(|env| crate::swarm::is_test_lease_env(env));
    if lease.is_some() && args != "doctor" {
        flags.extend(mode_flags(&invocation));
        return format!(
            "{} (berth removes a failed test runtime, so run the tests again)",
            render(&flags, "artisan test")
        );
    }
    if let Some(env) = service.runtime.env.as_deref().filter(|_| lease.is_none()) {
        flags.push(format!("--env {}", quote(env)));
    }
    flags.extend(mode_flags(&invocation));
    render(&flags, args)
}

/// The `berth logs` invocation that shows `service`'s output.
#[must_use]
pub(crate) fn logs(service: &ServiceConfig) -> String {
    for_service(service, &format!("logs --service {}", service.name))
}

/// The global flags that pick the engine, the project at `root` and a
/// preview, for a command a hint assembles itself (the swarm rerun).
#[must_use]
pub(crate) fn context_flags(root: &Path) -> Vec<String> {
    let invocation = invocation();
    let mut flags = engine_flag(&invocation);
    flags.push(project_flag(&invocation, root));
    flags.extend(mode_flags(&invocation));
    flags
}

fn mode_flags(invocation: &Invocation) -> Vec<String> {
    let mut flags = Vec::new();
    if invocation.dry_run {
        flags.push("--dry-run".to_owned());
    }
    if invocation.repro {
        flags.push("--repro".to_owned());
    }
    flags
}

/// `--engine` only when the run chose one: a project's configured engine
/// is read again from its file.
fn engine_flag(invocation: &Invocation) -> Vec<String> {
    match invocation.engine {
        Some(crate::config::ContainerEngine::Podman) => vec!["--engine podman".to_owned()],
        Some(_) => vec!["--engine docker".to_owned()],
        None => Vec::new(),
    }
}

/// `--config <file>` when the run's own config file is the project at
/// `root` (a file not named `.berth.yaml` is not found from its directory),
/// otherwise `--project-root <root>`.
fn project_flag(invocation: &Invocation, root: &Path) -> String {
    let root = absolute(root);
    if let Some(config) = &invocation.config {
        let config = absolute(config);
        if config.parent() == Some(root.as_path()) {
            return format!("--config {}", quote_path(&config));
        }
    }
    format!("--project-root {}", quote_path(&root))
}

/// The path with its directory made canonical, so a pasted command works
/// from any directory. The file name itself is kept: berth roots a project
/// at the directory of the file it was given, which for a symlinked
/// `.berth.yaml` is the link's, not its target's. The path as given when it
/// does not resolve (it is only shown).
fn absolute(path: &Path) -> PathBuf {
    if path.is_dir() {
        return std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    }
    let parent = match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent,
        _ => Path::new("."),
    };
    match (std::fs::canonicalize(parent), path.file_name()) {
        (Ok(parent), Some(name)) => parent.join(name),
        _ => path.to_path_buf(),
    }
}

fn render(flags: &[String], args: &str) -> String {
    if flags.is_empty() {
        format!("`berth {args}`")
    } else {
        format!("`berth {} {args}`", flags.join(" "))
    }
}

fn quote_path(path: &Path) -> String {
    quote(&path.display().to_string())
}

/// Quotes a value for a shell when it holds anything but safe characters.
#[must_use]
pub(crate) fn quote(value: &str) -> String {
    if !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"/._-:,".contains(&byte))
    {
        value.to_owned()
    } else {
        format!("'{}'", value.replace('\'', r"'\''"))
    }
}

#[cfg(test)]
mod tests {
    use super::{Invocation, command, for_service, record};

    fn database() -> crate::config::ServiceConfig {
        serde_json::from_value(serde_json::json!({
            "name": "db", "kind": "database", "driver": "postgres",
            "image": "postgres", "host": "127.0.0.1", "port": 5432
        }))
        .expect("service")
    }

    #[test]
    fn a_rerun_carries_the_invocations_own_flags() {
        record(Invocation {
            instance: Some("agent-2".to_owned()),
            env: Some("qa#2".to_owned()),
            project_root: Some("/work/my shop".into()),
            engine: Some(crate::config::ContainerEngine::Podman),
            ..Invocation::default()
        });
        assert_eq!(
            command("down --purge"),
            "`berth --engine podman --project-root '/work/my shop' --instance agent-2 --env \
             'qa#2' down --purge`"
        );
        record(Invocation {
            engine: Some(crate::config::ContainerEngine::Podman),
            instance: Some("agent-2".to_owned()),
            repro: true,
            ..Invocation::default()
        });
        assert_eq!(super::global("gc"), "`berth --engine podman gc`");
        // A project's configured podman reaches `gc` only through the flag.
        record(Invocation::default());
        crate::docker::with_container_engine(crate::config::ContainerEngine::Podman, || {
            assert_eq!(super::global("gc"), "`berth --engine podman gc`");
        });
        record(Invocation {
            engine: Some(crate::config::ContainerEngine::Podman),
            instance: Some("agent-2".to_owned()),
            repro: true,
            ..Invocation::default()
        });
        assert_eq!(
            command("up"),
            "`berth --engine podman --instance agent-2 --repro up`"
        );
        record(Invocation::default());
        assert_eq!(command("up"), "`berth up`");
    }

    #[test]
    fn health_limit_advice_never_says_to_leave_its_timeout_out() {
        record(Invocation {
            limit: super::Limit::HealthTimeout,
            ..Invocation::default()
        });
        let advice = super::raise_limit();
        assert!(advice.contains("`--timeout`"), "{advice}");
        assert!(!advice.contains("leave it out"), "{advice}");
        record(Invocation::default());
    }

    #[test]
    fn a_test_runtime_service_points_back_at_the_test_run() {
        record(Invocation::default());
        let mut service = database();
        service.runtime.root = Some("/work/bill".into());
        service.runtime.env = Some("testing-pool-0123456789abcdef-3".to_owned());
        service.runtime.instance = Some("agent-2".to_owned());
        let hint = for_service(&service, "logs --service db");
        assert!(
            hint.starts_with("`berth --project-root /work/bill --instance agent-2 artisan test`"),
            "{hint}"
        );
        assert!(!hint.contains("testing-pool"), "{hint}");
        let doctor = for_service(&service, "doctor");
        assert_eq!(
            doctor,
            "`berth --project-root /work/bill --instance agent-2 doctor`"
        );
    }

    #[test]
    fn a_service_command_names_its_project_instance_and_env() {
        record(Invocation::default());
        let mut service = database();
        service.runtime.root = Some("/work/bill".into());
        service.runtime.instance = Some("bill-1a2b3c".to_owned());
        assert_eq!(
            for_service(&service, "logs --service db"),
            "`berth --project-root /work/bill --instance bill-1a2b3c logs --service db`"
        );
    }

    #[test]
    fn a_service_of_the_runs_config_file_names_that_file_and_keeps_dry_run() {
        let root = crate::test_support::temp_root()
            .join(format!("berth-followup-config-{}", std::process::id()));
        std::fs::create_dir_all(root.join("infra")).expect("dir");
        let file = root.join("infra/dev.yaml");
        std::fs::write(&file, "").expect("config");
        record(Invocation {
            config: Some(file.clone()),
            dry_run: true,
            ..Invocation::default()
        });
        let mut service = database();
        service.runtime.root = Some(root.join("infra"));
        let canonical = std::fs::canonicalize(&file).expect("canonical");
        assert_eq!(
            for_service(&service, "up"),
            format!("`berth --config {} --dry-run up`", canonical.display())
        );
        record(Invocation::default());
        std::fs::remove_dir_all(&root).expect("cleanup");
    }
}
