//! Creates, adopts and readies shared server containers.

use anyhow::{Context, Result};
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::config::{SharedEngine, SharedTarget};

use super::{engine, postgres};

/// How long a process waits for another to finish creating a server.
const LOCK_TIMEOUT: Duration = Duration::from_secs(300);
/// How long a new server may take to accept authenticated connections.
const READY_TIMEOUT: Duration = Duration::from_secs(90);
/// The bounded retry for the init-server handoff: the official images start
/// a temporary server, run init scripts, stop it and start the real one, so
/// a probe can pass and the next connection fail (8.x FI-33).
const HANDOFF_TIMEOUT: Duration = Duration::from_secs(5);

/// Labels on every shared server and its volume.
fn labels(target: &SharedTarget, installation: &str) -> Vec<String> {
    [
        "dev.berth.managed=true".to_owned(),
        format!("dev.berth.installation={installation}"),
        "dev.berth.role=shared-server".to_owned(),
        format!("dev.berth.shared.engine={}", target.engine.as_str()),
        format!("dev.berth.shared.fingerprint={}", target.fingerprint),
    ]
    .into_iter()
    .flat_map(|label| ["--label".to_owned(), label])
    .collect()
}

/// Every shared server of this installation, as recorded by its labels.
///
/// # Errors
///
/// Returns an error when Docker cannot list containers or a server carries
/// an engine this berth does not know.
pub(super) fn installation_servers() -> Result<Vec<SharedTarget>> {
    let installation = crate::home::installation_id()?;
    let output = crate::docker::run_docker_output(
        &[
            "ps",
            "-a",
            "--filter",
            "label=dev.berth.role=shared-server",
            "--filter",
            &format!("label=dev.berth.installation={installation}"),
            "--format",
            "{{.Names}}\t{{.Label \"dev.berth.shared.engine\"}}\t{{.Label \"dev.berth.shared.fingerprint\"}}\t{{.Image}}",
        ],
        "failed to list shared servers",
    )?;
    if !output.status.success() {
        anyhow::bail!(
            "failed to list shared servers: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let fields: Vec<&str> = line.split('\t').collect();
            let [server, engine, fingerprint, image] = fields[..] else {
                anyhow::bail!("unexpected shared server listing '{line}'");
            };
            let engine = SharedEngine::from_label(engine)
                .with_context(|| format!("shared server {server}"))?;
            Ok(SharedTarget {
                engine,
                server: server.to_owned(),
                image: image.to_owned(),
                command: Vec::new(),
                fingerprint: fingerprint.to_owned(),
            })
        })
        .collect()
}

/// Holds the server's lock: creation, readiness and tenant provisioning of
/// one server never run concurrently.
pub(super) fn lock(target: &SharedTarget) -> Result<crate::home::HostLock> {
    crate::home::HostLock::acquire(&format!("shared-{}", target.server), LOCK_TIMEOUT)
}

/// The server's admin password, kept under `~/.berth/shared/{server}`.
pub(super) fn admin_secret(target: &SharedTarget) -> Result<String> {
    crate::home::read_or_create(
        &Path::new("shared").join(&target.server),
        "admin",
        crate::home::random_hex_secret,
        crate::home::is_hex_secret,
    )
}

/// Ensures the server runs and accepts connections. Must hold [`lock`].
///
/// # Errors
///
/// Returns an error when a container of that name belongs to someone else,
/// or the server cannot be created or does not become ready.
pub(super) fn ensure_running(target: &SharedTarget) -> Result<()> {
    let installation = crate::home::installation_id()?;
    match inspect(&target.server)? {
        Some(state) => {
            if state.installation != installation || state.fingerprint != target.fingerprint {
                anyhow::bail!(
                    "container {} exists but is not this installation's shared {} server \
                     (installation {}, fingerprint {}); remove it or use `isolation: dedicated`",
                    target.server,
                    target.engine.as_str(),
                    empty_dash(&state.installation),
                    empty_dash(&state.fingerprint)
                );
            }
            if !state.running {
                docker(
                    &["start", &target.server],
                    "failed to start the shared server",
                )?;
            }
        }
        None => create(target, &installation)?,
    }
    wait_ready(target)?;
    match target.engine {
        SharedEngine::Postgres => {
            let version = admin_sql(target, &postgres::server_version_sql())?;
            postgres::ensure_supported_version(version.trim())?;
            admin_sql(target, &postgres::harden_server_sql())?;
        }
        SharedEngine::Redis | SharedEngine::Valkey => {
            let info = admin_sql(target, &super::redis::server_info_commands())?;
            super::redis::ensure_supported_version(&info)?;
        }
        SharedEngine::Mysql | SharedEngine::Mariadb => {}
    }
    Ok(())
}

fn empty_dash(value: &str) -> &str {
    if value.is_empty() { "-" } else { value }
}

fn create(target: &SharedTarget, installation: &str) -> Result<()> {
    ensure_image(&target.image)?;
    let secret = admin_secret(target)?;
    let volume = format!("{}-data", target.server);
    let mut create_volume = vec!["volume".to_owned(), "create".to_owned()];
    create_volume.extend(labels(target, installation));
    create_volume.push(volume.clone());
    docker_owned(
        &create_volume,
        "failed to create the shared server's volume",
    )?;

    let mut run = vec![
        "run".to_owned(),
        "-d".to_owned(),
        "--name".to_owned(),
        target.server.clone(),
        "--restart".to_owned(),
        "unless-stopped".to_owned(),
    ];
    run.extend(labels(target, installation));
    let password_env = engine::server_password_env(target.engine);
    // The secret is passed by name from berth's environment, so it never
    // appears in berth's own argv or in `docker run`'s.
    if let Some(name) = password_env {
        run.extend(["-e".to_owned(), name.to_owned()]);
    }
    match target.engine {
        SharedEngine::Postgres => {
            run.extend([
                "-e".to_owned(),
                format!("POSTGRES_USER={}", postgres::ADMIN_USER),
                "-e".to_owned(),
                "POSTGRES_DB=postgres".to_owned(),
                "-v".to_owned(),
                format!("{volume}:{}", image_pgdata(&target.image, installation)?),
            ]);
        }
        SharedEngine::Mysql | SharedEngine::Mariadb => {
            run.extend(["-v".to_owned(), format!("{volume}:/var/lib/mysql")]);
        }
        SharedEngine::Redis | SharedEngine::Valkey => {
            seed_acl_file(target, &volume, installation, &secret)?;
            run.extend(["-v".to_owned(), format!("{volume}:/data")]);
        }
    }
    run.push(target.image.clone());
    run.extend(target.command.iter().cloned());
    let mut command = Command::new(crate::docker::docker_command());
    command.args(&run);
    if let Some(name) = password_env {
        command.env(name, &secret);
    }
    let output = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .context("failed to create the shared server")?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    // Another installation's process could not hold our lock; a name
    // conflict means another process of ours won the race (it had the lock
    // first and we read the state before it created the container).
    if stderr.contains("Conflict") || stderr.contains("already in use") {
        return Ok(());
    }
    anyhow::bail!(
        "failed to create shared server {}: {}",
        target.server,
        stderr.trim()
    )
}

/// The data directory the image declares. PostgreSQL-compatible images keep
/// data at different paths (`/var/lib/postgresql/data`, timescaledb-ha's
/// `/home/postgres/pgdata/data`), so the volume follows the image's `PGDATA`.
/// It is read from a throwaway container: Docker Desktop's containerd image
/// store reports an empty `Config` for some images in `image inspect`.
fn image_pgdata(image: &str, installation: &str) -> Result<String> {
    let args = [
        "run".to_owned(),
        "--rm".to_owned(),
        "--label".to_owned(),
        "dev.berth.managed=true".to_owned(),
        "--label".to_owned(),
        format!("dev.berth.installation={installation}"),
        "--label".to_owned(),
        "dev.berth.role=probe".to_owned(),
        "--entrypoint".to_owned(),
        "printenv".to_owned(),
        image.to_owned(),
        "PGDATA".to_owned(),
    ];
    let output =
        crate::docker::run_docker_output_owned(&args, "failed to read the image's PGDATA")?;
    let path = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if !output.status.success() || !path.starts_with('/') {
        anyhow::bail!(
            "image {image} does not declare PGDATA, so berth cannot keep a shared server's \
             data; use `isolation: dedicated`"
        );
    }
    Ok(path)
}

/// Writes the server's initial ACL file (the admin's password hash) into
/// its new volume, from stdin, before the server first starts.
fn seed_acl_file(
    target: &SharedTarget,
    volume: &str,
    installation: &str,
    secret: &str,
) -> Result<()> {
    let mut args = vec!["run".to_owned(), "--rm".to_owned(), "-i".to_owned()];
    args.extend([
        "--label".to_owned(),
        "dev.berth.managed=true".to_owned(),
        "--label".to_owned(),
        format!("dev.berth.installation={installation}"),
        "--label".to_owned(),
        "dev.berth.role=probe".to_owned(),
        "-v".to_owned(),
        format!("{volume}:/data"),
        "--entrypoint".to_owned(),
        "sh".to_owned(),
        target.image.clone(),
        "-c".to_owned(),
        format!("cat > {}", super::redis::ACL_FILE),
    ]);
    let mut child = Command::new(crate::docker::docker_command())
        .args(&args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("failed to seed the shared server's ACL file")?;
    if let Some(mut input) = child.stdin.take() {
        input
            .write_all(super::redis::initial_acl_file(secret).as_bytes())
            .context("failed to seed the shared server's ACL file")?;
    }
    let output = child
        .wait_with_output()
        .context("failed to seed the shared server's ACL file")?;
    if !output.status.success() {
        anyhow::bail!(
            "failed to seed the ACL file of {}: {}",
            target.server,
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(())
}

fn ensure_image(image: &str) -> Result<()> {
    let present =
        crate::docker::run_docker_output(&["image", "inspect", image], "failed to inspect image")?;
    if present.status.success() {
        return Ok(());
    }
    crate::docker::docker_pull(
        image,
        "failed to pull image",
        &format!("failed to pull {image}"),
    )
}

struct State {
    running: bool,
    installation: String,
    fingerprint: String,
}

fn inspect(server: &str) -> Result<Option<State>> {
    let output = crate::docker::run_docker_output(
        &[
            "inspect",
            "--format",
            "{{.State.Status}}\t{{index .Config.Labels \"dev.berth.installation\"}}\t{{index .Config.Labels \"dev.berth.shared.fingerprint\"}}",
            server,
        ],
        "failed to inspect the shared server",
    )?;
    if !output.status.success() {
        return Ok(None);
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let mut fields = text.trim().split('\t');
    Ok(Some(State {
        running: fields.next() == Some("running"),
        installation: fields.next().unwrap_or_default().replace("<no value>", ""),
        fingerprint: fields.next().unwrap_or_default().replace("<no value>", ""),
    }))
}

/// Waits until the server accepts an authenticated query, failing at once
/// when the container stops.
fn wait_ready(target: &SharedTarget) -> Result<()> {
    let started = Instant::now();
    loop {
        match inspect(&target.server)? {
            Some(state) if state.running => {}
            _ => anyhow::bail!(
                "shared server {} stopped while starting; see `docker logs {}`",
                target.server,
                target.server
            ),
        }
        match admin_sql(target, engine::ping(target.engine)) {
            Ok(_) => return Ok(()),
            // The data directory keeps the password it was created with; a
            // new admin secret never matches it, so waiting cannot help.
            // Removing the server would delete every instance's shared data,
            // so the recovery keeps it.
            Err(error) if engine::is_auth_failure(target.engine, &format!("{error:#}")) => {
                anyhow::bail!(
                    "shared server {server} rejects berth's admin password \
                     (~/.berth/shared/{server}/admin was replaced or lost). To keep the data, \
                     {recovery}, then run `berth up` again. Do not remove the server or its \
                     volume: it holds every instance's shared databases",
                    server = target.server,
                    recovery = engine::lost_secret_recovery(target.engine, &target.server)
                );
            }
            Err(_) => {}
        }
        if started.elapsed() >= READY_TIMEOUT {
            anyhow::bail!(
                "shared server {} did not accept connections within {}s; see `docker logs {}`",
                target.server,
                READY_TIMEOUT.as_secs(),
                target.server
            );
        }
        std::thread::sleep(Duration::from_millis(500));
    }
}

/// Runs SQL as the admin over TCP with the admin password, retrying only
/// the init-server handoff for a bounded few seconds. TCP matters for
/// readiness: the official images' temporary init server listens only on
/// the local socket, so only the final server answers here.
pub(super) fn admin_sql(target: &SharedTarget, sql: &str) -> Result<String> {
    let secret = admin_secret(target)?;
    let command = engine::admin_command(target.engine, &target.image);
    let started = Instant::now();
    loop {
        let (code, stdout, stderr) = exec_stdin(
            &target.server,
            engine::password_env(target.engine),
            &secret,
            &command,
            sql,
        )?;
        if code == Some(0) {
            return Ok(stdout);
        }
        let output = format!("{stdout}{stderr}");
        if !engine::is_handoff(target.engine, code, &output) || started.elapsed() >= HANDOFF_TIMEOUT
        {
            anyhow::bail!(
                "{} on {}: {}",
                target.engine.as_str(),
                target.server,
                output.trim()
            );
        }
        std::thread::sleep(Duration::from_millis(250));
    }
}

/// Runs the admin client with extra arguments and no input (for example
/// `redis-cli --scan`), failing on a non-zero exit.
pub(super) fn admin_cli(target: &SharedTarget, args: &[&str]) -> Result<String> {
    let secret = admin_secret(target)?;
    let mut command = engine::admin_command(target.engine, &target.image);
    command.extend_from_slice(args);
    let (code, stdout, stderr) = exec_stdin(
        &target.server,
        engine::password_env(target.engine),
        &secret,
        &command,
        "",
    )?;
    if code != Some(0) {
        anyhow::bail!(
            "{} on {}: {}{}",
            target.engine.as_str(),
            target.server,
            stdout.trim(),
            stderr.trim()
        );
    }
    Ok(stdout)
}

fn exec_stdin(
    container: &str,
    password_env: &str,
    password: &str,
    command: &[&str],
    stdin: &str,
) -> Result<(Option<i32>, String, String)> {
    // The password reaches the client by name from docker's environment,
    // never as an argument.
    let mut child = Command::new(crate::docker::docker_command())
        .args(["exec", "-i", "-e", password_env, container])
        .args(command)
        .env(password_env, password)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("failed to run a command in the shared server")?;
    if let Some(mut input) = child.stdin.take() {
        input
            .write_all(stdin.as_bytes())
            .context("failed to send SQL to the shared server")?;
    }
    let output = child
        .wait_with_output()
        .context("failed to wait for the shared server")?;
    Ok((
        output.status.code(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    ))
}

fn docker(args: &[&str], context: &str) -> Result<()> {
    let output = crate::docker::run_docker_output(args, context)?;
    if !output.status.success() {
        anyhow::bail!(
            "{context}: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(())
}

fn docker_owned(args: &[String], context: &str) -> Result<()> {
    let output = crate::docker::run_docker_output_owned(args, context)?;
    if !output.status.success() {
        anyhow::bail!(
            "{context}: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(())
}
