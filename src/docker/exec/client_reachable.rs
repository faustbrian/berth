//! Whether a restore's database client can reach its database before
//! anything is changed.

use anyhow::Result;

use crate::config::ServiceConfig;

use super::super::is_dry_run;
use super::{ensure_container_running, ensure_tenant_reachable, piped_client_command};

/// Fails unless the service's piped client could reach its database now: a
/// shared tenant's server running and attached, or the container running.
/// A restore asks before `--reset` drops anything; a `--reset` one also
/// accepts a database that does not exist yet, which the reset creates.
pub(crate) fn ensure_client_reachable(service: &ServiceConfig, reset: bool) -> Result<()> {
    if is_dry_run() {
        return Ok(());
    }
    if service.runtime.shared.is_some() {
        ensure_tenant_reachable(service)?;
    } else {
        ensure_container_running(service, &service.container_name()?)?;
    }
    // A running server may still refuse the client (its credentials, a
    // tenant's login, or the English messages a restore asks for): connect
    // as the restore will.
    let args = piped_client_command(service, false)?;
    let arg_refs = crate::docker::docker_arg_refs(&args);
    let mut child = crate::docker::spawn_docker_stdin_stdout_stderr_piped(
        &arg_refs,
        &crate::docker::runtime_command_error_context("exec"),
    )?;
    if let Some(mut stdin) = child.stdin.take() {
        // A client that already exited (refused) closed its stdin: its own
        // error, read below, says why.
        match std::io::Write::write_all(&mut stdin, b"SELECT 1;\n") {
            Err(error) if error.kind() != std::io::ErrorKind::BrokenPipe => {
                anyhow::bail!("failed to reach '{}': {error}", service.name)
            }
            _ => {}
        }
    }
    let output = child
        .wait_with_output()
        .map_err(|error| anyhow::anyhow!("failed to reach '{}': {error}", service.name))?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    let postgres = service.driver == crate::config::Driver::Postgres;
    if reset && !output.status.success() {
        // The reset creates a missing database; the server's version is
        // asked of its `postgres` database instead (shared servers run
        // PostgreSQL 13 or later).
        let maintenance = if postgres && service.runtime.shared.is_none() {
            Some(maintenance_probe(service)?)
        } else {
            None
        };
        let missing = reports_missing_database(service, &stderr)
            || maintenance.as_ref().is_some_and(|probe| probe.absent);
        if missing {
            match maintenance {
                Some(probe) if !probe.answered => anyhow::bail!(
                    "service '{}' has no database '{}' yet, and berth could not ask its \
                     `postgres` database which server it runs, so nothing was changed: {}{}",
                    service.name,
                    service.database.as_deref().unwrap_or("app"),
                    probe.stderr.trim(),
                    superuser_hint(&probe.stderr)
                ),
                Some(probe) if !probe.checks_hold => return Err(old_server(service)),
                _ => return Ok(()),
            }
        }
    }
    // The probe runs berth's restore checks too: a server where they cannot
    // hold (one whose start query returns no row) is refused before any
    // restore, `--reset` or not, changes anything.
    if postgres && output.status.success() && !has_start_line(&output.stdout) {
        return Err(old_server(service));
    }
    if !output.status.success() || stderr.contains("ERROR:") || stderr.contains("FATAL:") {
        anyhow::bail!(
            "service '{}' refuses berth's database client, so nothing was changed: {}{}",
            service.name,
            stderr.trim(),
            superuser_hint(&stderr)
        );
    }
    Ok(())
}

/// A restore asks the server for English errors (`lc_messages`), which only
/// a superuser may set.
fn superuser_hint(stderr: &str) -> &'static str {
    if stderr.contains("permission denied to set parameter \"lc_messages\"") {
        "; a restore needs the service's `username` to be a superuser (the image's \
         POSTGRES_USER)"
    } else {
        ""
    }
}

/// berth's start query printed its line: the server is one its restore
/// checks hold on.
fn has_start_line(stdout: &[u8]) -> bool {
    String::from_utf8_lossy(stdout)
        .lines()
        .any(|line| line.trim_start().starts_with("berth-backend-start "))
}

fn old_server(service: &ServiceConfig) -> anyhow::Error {
    anyhow::anyhow!(
        "service '{}' runs a PostgreSQL server older than 10.7, or 11.0 or 11.1, on which a dump \
         can hide its errors from berth's checks, so nothing was changed: use a newer image",
        service.name
    )
}

/// What a dedicated PostgreSQL server's `postgres` database says.
struct MaintenanceProbe {
    /// Its client connected and ran the query.
    answered: bool,
    /// The service's database is not in its catalog.
    absent: bool,
    /// berth's start query printed its line (see [`has_start_line`]).
    checks_hold: bool,
    stderr: String,
}

/// Asks a dedicated PostgreSQL server's `postgres` database whether its
/// catalog lacks the service's database, through the restore's own client
/// arguments: the server says a database is missing before it applies the
/// client's `lc_messages`, so in its own language, which
/// [`reports_missing_database`] may not read.
fn maintenance_probe(service: &ServiceConfig) -> Result<MaintenanceProbe> {
    let database = service.database.as_deref().unwrap_or("app");
    let mut maintenance = service.clone();
    maintenance.database = Some("postgres".to_owned());
    let args = piped_client_command(&maintenance, false)?;
    let arg_refs = crate::docker::docker_arg_refs(&args);
    let mut child = crate::docker::spawn_docker_stdin_stdout_stderr_piped(
        &arg_refs,
        &crate::docker::runtime_command_error_context("exec"),
    )?;
    let sql = format!(
        "SELECT 'berth: absent' WHERE NOT EXISTS (SELECT FROM pg_database WHERE datname = '{}');\n",
        database.replace('\'', "''")
    );
    if let Some(mut stdin) = child.stdin.take() {
        match std::io::Write::write_all(&mut stdin, sql.as_bytes()) {
            Err(error) if error.kind() != std::io::ErrorKind::BrokenPipe => {
                anyhow::bail!("failed to reach '{}': {error}", service.name)
            }
            _ => {}
        }
    }
    let output = child
        .wait_with_output()
        .map_err(|error| anyhow::anyhow!("failed to reach '{}': {error}", service.name))?;
    let answered = output.status.success();
    Ok(MaintenanceProbe {
        answered,
        absent: answered
            && String::from_utf8_lossy(&output.stdout)
                .lines()
                .any(|line| line.trim() == "berth: absent"),
        checks_hold: answered && has_start_line(&output.stdout),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    })
}

/// Whether the client's error says the service's database does not exist
/// (PostgreSQL: `database "x" does not exist`; MySQL and MariaDB: error
/// 1049, `Unknown database 'x'`).
pub(super) fn reports_missing_database(service: &ServiceConfig, stderr: &str) -> bool {
    // The client's own fallback name (`piped_client_args`), which a reset
    // recreates too.
    let database = service.database.as_deref().unwrap_or("app");
    stderr.contains(&format!("database \"{database}\" does not exist"))
        // MySQL and MariaDB print the error's number whatever the language.
        || stderr.contains("ERROR 1049 (42000)")
        || stderr.contains(&format!("Unknown database '{database}'"))
}
