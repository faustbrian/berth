//! docker ops module.
//!
//! Contains ad-hoc Docker command operations used by Berth command workflows.

use anyhow::Result;

use crate::config::ServiceConfig;

mod attach;
mod common;
mod cp;
mod events;
mod inspect;
mod kill;
mod pause;
mod port;
mod prune;
mod stats;
mod top;
mod unpause;
mod wait;

pub fn top(service: &ServiceConfig, top_args: &[String]) -> Result<()> {
    top::top(service, top_args)
}

pub struct StatsOptions<'a> {
    pub no_stream: bool,
    pub format: Option<&'a str>,
}

pub fn stats(service: &ServiceConfig, options: StatsOptions<'_>) -> Result<()> {
    stats::stats(service, options)
}

pub fn inspect_container(
    service: &ServiceConfig,
    format: Option<&str>,
    size: bool,
    object_type: Option<&str>,
) -> Result<()> {
    // A shared server's configuration holds its admin secret.
    crate::shared::refuse_server_operation(service, "inspect")?;
    inspect::inspect_container(service, format, size, object_type)
}

pub fn attach(
    service: &ServiceConfig,
    no_stdin: bool,
    sig_proxy: bool,
    detach_keys: Option<&str>,
) -> Result<()> {
    crate::shared::refuse_server_operation(service, "attach to")?;
    attach::attach(service, no_stdin, sig_proxy, detach_keys)
}

pub struct CpOptions {
    pub follow_link: bool,
    pub archive: bool,
}

pub fn cp(source: &str, destination: &str, options: CpOptions) -> Result<()> {
    cp::cp(source, destination, options)
}

/// Kills the service's container. A shared-backed service only leaves the
/// instance network, as with `stop`: the server keeps serving other
/// instances.
pub fn kill(service: &ServiceConfig, signal: Option<&str>) -> Result<()> {
    if service.runtime.shared.is_some() {
        return crate::shared::detach(service);
    }
    kill::kill(service, signal)
}

pub fn pause(service: &ServiceConfig) -> Result<()> {
    crate::shared::refuse_server_operation(service, "pause")?;
    pause::pause(service)
}

pub fn unpause(service: &ServiceConfig) -> Result<()> {
    crate::shared::refuse_server_operation(service, "unpause")?;
    unpause::unpause(service)
}

pub fn wait(service: &ServiceConfig, condition: Option<&str>) -> Result<()> {
    wait::wait(service, condition)
}

pub fn events(
    since: Option<&str>,
    until: Option<&str>,
    format: Option<&str>,
    filters: &[String],
) -> Result<()> {
    events::events(since, until, format, filters)
}

pub fn port(service: &ServiceConfig, private_port: Option<&str>) -> Result<()> {
    port::port(service, private_port)
}

pub fn port_output(service: &ServiceConfig, private_port: Option<&str>) -> Result<String> {
    port::port_output(service, private_port)
}

pub struct PruneOptions<'a> {
    pub force: bool,
    pub filters: &'a [String],
}

pub fn prune(options: PruneOptions<'_>) -> Result<()> {
    prune::prune(options)
}

pub fn prune_stopped_container(service: &ServiceConfig) -> Result<()> {
    prune::prune_stopped_container(service)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io::Write;
    use std::time::SystemTime;
    use std::time::UNIX_EPOCH;

    use crate::config::{Driver, Kind, ServiceConfig};
    use crate::docker;
    use crate::docker::ops::PruneOptions;

    fn service(name: &str, kind: Kind, driver: Driver) -> ServiceConfig {
        ServiceConfig {
            name: name.to_owned(),
            kind,
            driver,
            image: "postgres:16".to_owned(),
            host: "127.0.0.1".to_owned(),
            port: 5432,
            database: None,
            username: None,
            password: None,
            bucket: None,
            access_key: None,
            secret_key: None,
            api_key: None,
            region: None,
            scheme: None,
            domain: None,
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
            container_name: Some(name.to_owned()),
            resolved_container_name: Some(name.to_owned()),
            runtime: Default::default(),
        }
    }

    fn with_fake_docker<F, T>(script: &str, test: F) -> T
    where
        F: FnOnce() -> T,
    {
        let bin_dir = crate::test_support::temp_root().join(format!(
            "berth-ops-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        // A clock tick is coarse: two parallel tests could share a name and
        // remove each other's fake binary.
        let bin_dir = docker::unique_test_path(bin_dir);
        fs::create_dir_all(&bin_dir).expect("fake docker dir");
        let binary = bin_dir.join("docker");
        let mut file = fs::File::create(&binary).expect("write fake docker");
        writeln!(file, "#!/bin/sh\n{}", script).expect("script");
        drop(file);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&binary)
                .expect("binary permissions")
                .permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&binary, perms).expect("chmod");
        }

        let result = docker::with_dry_run_state(false, || {
            docker::with_docker_command(&binary.to_string_lossy(), test)
        });
        fs::remove_dir_all(&bin_dir).ok();
        result
    }

    #[test]
    fn attach_builds_args_and_executes_docker() {
        let db = service("db", Kind::Database, Driver::Postgres);
        with_fake_docker("exit 0", || {
            super::attach(&db, false, false, None).expect("attach");
        });
    }

    #[test]
    fn attach_supports_optional_flags() {
        let db = service("db", Kind::Database, Driver::Postgres);
        with_fake_docker("exit 0", || {
            super::attach(&db, true, true, Some("ctrl-c")).expect("attach with options");
        });
    }

    #[test]
    fn cp_supports_copy_options() {
        with_fake_docker("exit 0", || {
            super::cp(
                "src",
                "dst",
                super::CpOptions {
                    follow_link: true,
                    archive: true,
                },
            )
            .expect("cp");
        });
    }

    #[test]
    fn events_builds_filter_args() {
        with_fake_docker("exit 0", || {
            super::events(
                Some("2024-01-01T00:00:00Z"),
                Some("2024-01-01T01:00:00Z"),
                Some("json"),
                &["name=db".to_owned()],
            )
            .expect("events");
        });
    }

    #[test]
    fn inspect_container_prints_optional_flags() {
        let db = service("db", Kind::Database, Driver::Postgres);
        with_fake_docker("exit 0", || {
            super::inspect_container(&db, Some("{{.ID}}"), true, Some("container"))
                .expect("inspect");
        });
    }

    #[test]
    fn kill_passes_optional_signal() {
        let db = service("db", Kind::Database, Driver::Postgres);
        with_fake_docker("exit 0", || {
            super::kill(&db, Some("SIGKILL")).expect("kill");
        });
    }

    #[test]
    fn pause_and_unpause_delegate_container_commands() {
        let db = service("db", Kind::Database, Driver::Postgres);
        with_fake_docker("exit 0", || {
            super::pause(&db).expect("pause");
            super::unpause(&db).expect("unpause");
        });
    }

    #[test]
    fn wait_passes_optional_condition() {
        let db = service("db", Kind::Database, Driver::Postgres);
        with_fake_docker("exit 0", || {
            super::wait(&db, Some("not-running")).expect("wait");
        });
    }

    #[test]
    fn stats_adds_format_flags() {
        let db = service("db", Kind::Database, Driver::Postgres);
        with_fake_docker("exit 0", || {
            super::stats(
                &db,
                super::StatsOptions {
                    no_stream: true,
                    format: Some("{{.CPUPerc}}"),
                },
            )
            .expect("stats");
        });
    }

    #[test]
    fn top_appends_user_args() {
        let db = service("db", Kind::Database, Driver::Postgres);
        with_fake_docker("exit 0", || {
            super::top(&db, &["aux".to_owned()]).expect("top");
        });
    }

    #[test]
    fn port_outputs_publish_mapping_when_not_dry_run() {
        let db = service("db", Kind::Database, Driver::Postgres);
        with_fake_docker("printf '0.0.0.0:5432 -> 127.0.0.1:5432\\n'; exit 0", || {
            let output = super::port_output(&db, Some("5432/tcp")).expect("port output");
            assert_eq!(output, "0.0.0.0:5432 -> 127.0.0.1:5432");
        });
    }

    #[test]
    fn prune_dry_run_logs_candidates_when_filters_present() {
        let _app = service("app", Kind::App, Driver::Frankenphp);
        docker::with_dry_run_lock(|| {
            with_fake_docker("printf 'alpha\nbeta\n'; exit 0", || {
                super::prune(PruneOptions {
                    force: false,
                    filters: &["label=berth=true".to_owned()],
                })
                .expect("dry run prune");
            });
        });
    }

    #[test]
    fn prune_stopped_container_skips_when_dry_run() {
        let db = service("db", Kind::Database, Driver::Postgres);
        docker::with_dry_run_lock(|| {
            super::prune_stopped_container(&db).expect("prune stopped");
        });
    }
}
