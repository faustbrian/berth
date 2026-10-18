//! Creates, adopts and readies shared server containers.

use anyhow::{Context, Result};
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;

use crate::config::{SharedEngine, SharedTarget};

use super::{engine, postgres};

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
            &format!(
                "{{{{.Names}}}}\t{}\t{}\t{{{{.Image}}}}",
                crate::docker::label_template("dev.berth.shared.engine"),
                crate::docker::label_template("dev.berth.shared.fingerprint")
            ),
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
    let name = format!("shared-{}", target.server);
    if super::best_effort::is_best_effort() {
        return crate::home::HostLock::try_acquire(&name)?.ok_or_else(|| {
            anyhow::anyhow!(
                "skipped shared server {}: another berth process is using it",
                target.server
            )
        });
    }
    crate::home::HostLock::acquire(&name)
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
            // Only a target from a project file can recreate the server: one
            // read back from labels (by `gc` or a purge, which reach it with
            // `exec` alone) has no run options, and every engine's options
            // (`--aclfile`, connection limits) or an authored command make a
            // project file's `command` non-empty.
            if !target.command.is_empty()
                && super::server_network::needs_attachable_recreation(&target.server)?
            {
                // Its data volume and admin secret are kept and reused.
                docker(
                    &["rm", "-f", &target.server],
                    "failed to replace the shared server",
                )?;
                create(target, &installation)?;
            } else if !state.running {
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
    // A recreated server keeps the volume it had.
    if !crate::docker::volume_exists(&volume)? {
        let mut create_volume = vec!["volume".to_owned(), "create".to_owned()];
        create_volume.extend(labels(target, installation));
        create_volume.push(volume.clone());
        docker_owned(
            &create_volume,
            "failed to create the shared server's volume",
        )?;
    }

    let mut run = vec![
        "run".to_owned(),
        "-d".to_owned(),
        "--name".to_owned(),
        target.server.clone(),
        "--restart".to_owned(),
        "unless-stopped".to_owned(),
    ];
    run.extend(labels(target, installation));
    // Instance networks are attached after it starts.
    run.extend(crate::docker::attachable_network_args());
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
/// its volume, from stdin, before the server first starts; a volume that
/// already holds one keeps it.
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
        super::acl_seed_script::acl_seed_script(super::redis::ACL_FILE),
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
/// when the container stops or rejects the admin password. There is no
/// deadline: a first start on a loaded machine is slow, not broken.
fn wait_ready(target: &SharedTarget) -> Result<()> {
    let deadline = crate::wait::Deadline::start();
    loop {
        crate::parallel::ensure_no_sibling_failed()?;
        deadline.ensure(&format!("shared server {}", target.server))?;
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
            // Reaping in passing never waits for a server still starting.
            Err(error) if super::best_effort::is_best_effort() => {
                return Err(error.context(format!(
                    "skipped shared server {}: it does not answer yet",
                    target.server
                )));
            }
            // `admin_sql` retries the init handoff itself, so any other
            // error is permanent, unless the server stopped meanwhile.
            // The engine failing to run the ping says nothing about the
            // server: ask again.
            Err(error) if engine::is_engine_exec_failure(&format!("{error:#}")) => {}
            Err(error) => {
                if inspect(&target.server)?.is_some_and(|state| state.running) {
                    return Err(error.context(format!(
                        "shared server {} does not accept berth's admin connection",
                        target.server
                    )));
                }
            }
        }
        std::thread::sleep(Duration::from_millis(500));
    }
}

/// Runs SQL as the admin over TCP with the admin password, retrying only
/// the init-server handoff while the server runs: the official images start
/// a temporary server, run init scripts, stop it and start the real one, so a
/// probe can pass and the next connection fail (8.x FI-33). TCP matters for
/// readiness: the temporary init server listens only on the local socket, so
/// only the final server answers here.
pub(super) fn admin_sql(target: &SharedTarget, sql: &str) -> Result<String> {
    let secret = admin_secret(target)?;
    let command = engine::admin_command(target.engine, &target.image);
    let deadline = crate::wait::Deadline::start();
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
        // A failed `inspect` is no answer, so the handoff wait goes on.
        let handing_off = engine::is_handoff(target.engine, code, &output)
            && !matches!(
                crate::docker::container_state(&target.server),
                crate::docker::ContainerState::NotRunning(_)
                    | crate::docker::ContainerState::Missing
            );
        // Reaping in passing never waits for a server still starting.
        if handing_off && !super::best_effort::is_best_effort() {
            crate::parallel::ensure_no_sibling_failed()?;
            deadline.ensure(&format!("shared server {}", target.server))?;
        } else {
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

/// The `sh` loop `admin_scan` runs: `$1` is the database count, the other
/// arguments the key prefixes. Keys come back one per line behind their
/// database number; a client failure ends the scan with its exit code.
fn scan_script(client: &str) -> String {
    format!(
        "d=$1; shift; n=0\n\
         while [ \"$n\" -lt \"$d\" ]; do\n\
         for p in \"$@\"; do\n\
         out=$({client} -n \"$n\" --scan --pattern \"$p*\") || exit 1\n\
         [ -z \"$out\" ] || printf '%s\\n' \"$out\" | while IFS= read -r k; do printf '%s\\t%s\\n' \"$n\" \"$k\"; done\n\
         done\n\
         n=$((n + 1))\n\
         done"
    )
}

/// Lists the keys matching each prefix in every logical database of a Redis
/// or Valkey server, as `(database, key)` pairs, in one `exec`: the server's
/// lock is held meanwhile, and one `exec` per database (16 by default, each
/// several seconds on a loaded engine) kept every other instance waiting for
/// minutes.
pub(super) fn admin_scan(
    target: &SharedTarget,
    databases: u32,
    prefixes: &[String],
) -> Result<Vec<(u32, String)>> {
    let secret = admin_secret(target)?;
    let client = engine::admin_command(target.engine, &target.image)
        .iter()
        .map(|word| format!("'{word}'"))
        .collect::<Vec<_>>()
        .join(" ");
    let script = scan_script(&client);
    let count = databases.to_string();
    let mut command = vec!["sh", "-c", script.as_str(), "sh", count.as_str()];
    command.extend(prefixes.iter().map(String::as_str));
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
    stdout
        .lines()
        .filter_map(|line| line.split_once('\t'))
        .map(|(database, key)| {
            database
                .parse::<u32>()
                .map(|database| (database, key.to_owned()))
                .map_err(|_| {
                    anyhow::anyhow!(
                        "unexpected scan line from {}: {line}",
                        target.server,
                        line = database
                    )
                })
        })
        .collect()
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

#[cfg(test)]
mod scan_script_tests {
    use super::scan_script;

    /// Runs the script with a fake client that answers `-n N --scan
    /// --pattern P` from `keys` (`database pattern-prefix key` triples), or
    /// fails when `fail` is set.
    fn run(keys: &str, fail: bool, args: &[&str]) -> std::process::Output {
        let dir =
            crate::docker::unique_test_path(crate::test_support::temp_root().join("scan-script"));
        std::fs::create_dir_all(&dir).expect("dir");
        let client = dir.join("redis-cli");
        std::fs::write(&client, format!(
            "#!/bin/sh\n[ -n \"{fail}\" ] && exit 1\nn=$2; p=${{5%\\*}}\nprintf '%s\\n' \"{keys}\" | while read -r db pre key; do [ \"$db\" = \"$n\" ] && [ \"$pre\" = \"$p\" ] && echo \"$key\"; done\nexit 0\n",
            fail = if fail { "1" } else { "" },
        ))
        .expect("fake client");
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&client, std::fs::Permissions::from_mode(0o755))
                .expect("chmod");
        }
        let script = scan_script(&format!("'{}'", client.display()));
        std::process::Command::new("sh")
            .arg("-c")
            .arg(&script)
            .arg("sh")
            .args(args)
            .output()
            .expect("run sh")
    }

    #[test]
    fn every_database_and_prefix_is_scanned_in_one_run() {
        let output = run(
            "0 a: a:x\n1 a: a:y\n2 b: b:z\n3 a: a:outside",
            false,
            &["3", "a:", "b:"],
        );
        assert!(output.status.success());
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            "0\ta:x\n1\ta:y\n2\tb:z\n",
            "databases 0 to 2, both prefixes, nothing past the count"
        );
    }

    #[test]
    fn a_failing_client_fails_the_scan() {
        let output = run("", true, &["2", "a:"]);
        assert_eq!(output.status.code(), Some(1));
    }
}
