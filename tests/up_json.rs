//! `berth up --json` through the real binary, with a fake `docker` and a
//! fake `curl` first on PATH.

// Integration tests are their own crate: panicking on setup failures and
// top-level `#[test]` functions are the idiom here.
#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::tests_outside_test_module
)]

use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::{Command, Output};

/// `FAKE_BOOT=ok|fatal|rendered|exit|restarting|restarting-twice`: how
/// `php artisan about` answers (`rendered` is an exception Laravel's console
/// kernel rendered, `exit` stops the app container, `restarting` is the
/// engine refusing the exec, `restarting-twice` only for the first two).
/// `FAKE_DB_HEALTH=exit` stops the database during its health check, and
/// `FAKE_CACHE_HEALTH=never` keeps the cache from ever answering. `FAKE_DB_RUN=busy` makes the database's `run` fail.
const FAKE_DOCKER: &str = r#"#!/bin/sh
# The gateway reload sends its config on stdin; take it all.
case "$*" in *"caddy reload"*) cat > /dev/null ;; esac
S="$FAKE_STATE"
echo "$*" >> "$S/log"
case "$1 $2" in
  "network inspect")
    [ -f "$S/network" ] || exit 1
    case "$*" in *--format*) cat "$BERTH_HOME/installation-id" ;; esac
    exit 0 ;;
  "network create") touch "$S/network"; exit 0 ;;
  "network rm") rm -f "$S/network"; exit 0 ;;
  "network connect"|"network disconnect") exit 0 ;;
  "image inspect")
    # With FAKE_DERIVED, a derived image exists only once a build made it.
    case "$*" in *berth/serve-*) [ -n "$FAKE_DERIVED" ] && [ ! -f "$S/built" ] && exit 1 ;; esac
    [ -n "$FAKE_COLD" ] && [ ! -f "$S/pulled" ] && exit 1; exit 0 ;;
esac
case "$1" in
  build) [ -n "$FAKE_DERIVED" ] && { echo build >> "$S/builds"; sleep 1; touch "$S/built"; }; exit 0 ;;
  pull) echo "17-alpine: Pulling from library/postgres"; echo "Status: Downloaded newer image"; touch "$S/pulled"; exit 0 ;;
  run)
    name=""; prev=""
    for arg in "$@"; do [ "$prev" = "--name" ] && name="$arg"; prev="$arg"; done
    case "$name" in *_db) [ "$FAKE_DB_RUN" = busy ] && { echo "port is already allocated" >&2; exit 125; } ;; esac
    mkdir -p "$S/c"; echo running > "$S/c/$name"; exit 0 ;;
  rm) for arg in "$@"; do rm -f "$S/c/$arg"; done; exit 0 ;;
  logs)
    name=""; for arg in "$@"; do name="$arg"; done
    [ -f "$S/c/$name" ] || { echo "Error response from daemon: No such container: $name" >&2; exit 1; }
    printf 'ready to accept connections\n\377bad\nafter\n'; echo "a warning" >&2; exit 0 ;;
  inspect)
    name=""; for arg in "$@"; do name="$arg"; done
    [ -f "$S/c/$name" ] || { echo "Error: No such object: $name" >&2; exit 1; }
    case "$*" in
      *NetworkSettings.Networks*) printf '{}' ;;
      *dev.berth.installation*) cat "$BERTH_HOME/installation-id" ;;
      *--format*) cat "$S/c/$name" | tr -d '\n' ;;
      *) printf '[{"State":{"Status":"running"},"NetworkSettings":{"Ports":{}},"HostConfig":{"PortBindings":{}}}]' ;;
    esac
    exit 0 ;;
  exec)
    case "$*" in
      *"artisan about"*)
        case "$FAKE_BOOT" in
          fatal) echo "PHP Fatal error:  Uncaught Error in /app/bootstrap/app.php:9" >&2; exit 255 ;;
          exit) for f in "$S"/c/*_app; do echo exited > "$f"; done; exit 1 ;;
          undefined) echo 'Command "about" is not defined.' >&2; exit 1 ;;
          rendered) echo '   ParseError  syntax error, unexpected token "}" at routes/console.php:12' >&2; exit 1 ;;
          restarting) echo 'Error response from daemon: Container x is restarting, wait until the container is running' >&2; exit 1 ;;
          hang) sleep 60 ;;
          oci-then-ok)
            [ -f "$S/oci" ] || { touch "$S/oci"; echo 'OCI runtime exec failed: exec failed: read init-p: connection reset by peer: unknown' >&2; exit 126; } ;;
          restarting-twice)
            n=$(cat "$S/boots" 2>/dev/null || echo 0); n=$((n + 1)); echo "$n" > "$S/boots"
            [ "$n" -le 2 ] && { echo 'Error response from daemon: Container x is restarting, wait until the container is running' >&2; exit 1; } ;;
        esac
        echo "Environment ... local"; exit 0 ;;
      *wget*) printf 'ok'; echo "  HTTP/1.1 200 OK" >&2 ;;
      *pg_isready*) [ "$FAKE_DB_HEALTH" = exit ] && { for f in "$S"/c/*_db; do echo exited > "$f"; done; exit 1; } ;;
      *redis-cli*) [ "$FAKE_CACHE_HEALTH" = never ] && exit 1 ;;
      *"horizon:status"*)
        case "$FAKE_HORIZON" in
          broken) echo '   ErrorException  Undefined array key "redis" in config/horizon.php:12' >&2; exit 1 ;;
          inactive-twice)
            n=$(cat "$S/horizon" 2>/dev/null || echo 0); n=$((n + 1)); echo "$n" > "$S/horizon"
            [ "$n" -le 2 ] && { echo 'Horizon is inactive.'; exit 1; } ;;
        esac ;;
      *" -i "*) cat >/dev/null; echo "SET"; echo "CREATE TABLE" ;;
    esac
    exit 0 ;;
esac
exit 0
"#;

const FAKE_CURL: &str = "#!/bin/sh\nprintf 'ok\\n200'\n";

const PROJECT: &str = "schema_version: 9\nproject: shop\nproject_type: project\nservices:\n  db:\n    preset: postgres\n    isolation: dedicated\n    password: canary-s3cret-value\n    env:\n      DATABASE_URL: postgres://u:canary-s3cret-value@db/x\n  app:\n    preset: laravel\n    hook:\n      - name: greet\n        phase: post_up\n        run:\n          type: script\n          path: hook.sh\n";

/// Two free loopback ports for this fixture's gateway, so parallel test
/// binaries (and a real gateway on the developer's machine) never contend
/// for the default 8443/8080. They come from 20000–39999, below the OS
/// ephemeral range, and each is claimed with an exclusive marker file, so
/// fixtures in other test processes never pick the same port (berth binds
/// it briefly to check that it is free). [`release_ports`] drops the claims.
fn free_ports() -> (u16, u16) {
    let claims = std::env::temp_dir().join("berth-test-port-claims");
    std::fs::create_dir_all(&claims).expect("claims dir");
    let mut port = 20_000 + u16::try_from(std::process::id() % 20_000).expect("below 20000");
    let mut pick = || loop {
        port = 20_000 + (port - 20_000 + 7_919) % 20_000;
        let claimed = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(claims.join(port.to_string()))
            .is_ok();
        // A connect probe, not a bind: macOS sets close-on-exec on a new socket
        // only after creating it, so a listener bound here can leak into a
        // `berth` another test thread spawns at that instant and keep the
        // port bound, failing that fixture's gateway with "port in use".
        // A refused connection means nothing listens there.
        if claimed && std::net::TcpStream::connect(("127.0.0.1", port)).is_err() {
            return port;
        }
    };
    let https = pick();
    (https, pick())
}

fn release_ports(ports: (u16, u16)) {
    let claims = std::env::temp_dir().join("berth-test-port-claims");
    for port in [ports.0, ports.1] {
        drop(std::fs::remove_file(claims.join(port.to_string())));
    }
}

struct Fixture {
    base: PathBuf,
    ports: (u16, u16),
}

impl Fixture {
    fn new(label: &str) -> Self {
        let base =
            std::env::temp_dir().join(format!("berth-upjson-{label}-{}", std::process::id()));
        drop(std::fs::remove_dir_all(&base));
        for dir in ["project", "bin", "state/c", "home", "user-home"] {
            std::fs::create_dir_all(base.join(dir)).expect("fixture dir");
        }
        std::fs::write(base.join("project/.berth.yaml"), PROJECT).expect("project file");
        std::fs::write(base.join("project/artisan"), "#!/usr/bin/env php\n").expect("artisan");
        std::fs::write(base.join("project/hook.sh"), "echo HOOK-OUTPUT\n").expect("hook");
        for (name, script) in [("docker", FAKE_DOCKER), ("curl", FAKE_CURL)] {
            let path = base.join("bin").join(name);
            std::fs::write(&path, script).expect("fake");
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).expect("chmod");
        }
        Self {
            base,
            ports: free_ports(),
        }
    }

    fn berth(&self, args: &[&str], env: &[(&str, &str)]) -> Output {
        let mut command = Command::new(env!("CARGO_BIN_EXE_berth"));
        command
            .current_dir(self.base.join("project"))
            .args(args)
            .env(
                "PATH",
                format!(
                    "{}:{}",
                    self.base.join("bin").display(),
                    std::env::var("PATH").unwrap_or_default()
                ),
            )
            .env("HOME", self.base.join("user-home"))
            .env("BERTH_HOME", self.base.join("home"))
            .env("BERTH_GATEWAY_HTTPS_PORT", self.ports.0.to_string())
            .env("BERTH_GATEWAY_HTTP_PORT", self.ports.1.to_string())
            .env("FAKE_STATE", self.base.join("state"))
            .env_remove("BERTH_INSTANCE")
            .env_remove("BERTH_ENGINE");
        for pair in env {
            command.env(pair.0, pair.1);
        }
        bounded_output(&mut command)
    }

    fn write_project(&self, contents: &str) {
        std::fs::write(self.base.join("project/.berth.yaml"), contents).expect("project file");
    }

    fn logs(&self) -> String {
        let dir = self.base.join("user-home/.config/berth/logs");
        std::fs::read_dir(&dir)
            .map(|entries| {
                entries
                    .filter_map(Result::ok)
                    .map(|entry| std::fs::read_to_string(entry.path()).unwrap_or_default())
                    .collect()
            })
            .unwrap_or_default()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        release_ports(self.ports);
        drop(std::fs::remove_dir_all(&self.base));
    }
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// stdout holds exactly one JSON value (and a trailing newline).
fn one_document(stdout: &[u8]) -> serde_json::Value {
    let mut stream = serde_json::Deserializer::from_slice(stdout).into_iter::<serde_json::Value>();
    let document = stream.next().expect("a document").expect("valid JSON");
    assert!(
        stream.next().is_none(),
        "exactly one document: {}",
        text(stdout)
    );
    document
}

#[test]
fn up_json_prints_one_ready_document_and_keeps_progress_and_hooks_on_stderr() {
    let fixture = Fixture::new("ready");
    let up = fixture.berth(&["up", "--json"], &[]);
    assert!(up.status.success(), "{}", text(&up.stderr));
    let document = one_document(&up.stdout);
    assert_eq!(document["status"], "ready");
    assert_eq!(document["schema"], 1);
    assert_eq!(document["project"], "shop");
    assert!(
        document["instance"]
            .as_str()
            .is_some_and(|id| !id.is_empty())
    );
    let urls = document["urls"].as_array().expect("urls");
    assert!(urls.iter().any(|url| {
        url.as_str()
            .is_some_and(|text| text.ends_with(&format!(".berth.localhost:{}", fixture.ports.0)))
    }));
    let services = document["services"].as_array().expect("services");
    let db = services.iter().find(|svc| svc["name"] == "db").expect("db");
    assert_eq!(db["health"], "healthy");
    assert_eq!(db["endpoint"], "db:5432");
    assert_eq!(db["published"], false);
    let app = services
        .iter()
        .find(|svc| svc["name"] == "app")
        .expect("app");
    assert_eq!(app["health"], "healthy", "the boot probe ran");
    assert_eq!(
        document["credentials"]["db"]["password"],
        "canary-s3cret-value"
    );
    assert_eq!(document["environment"]["DB_HOST"], "db");
    assert!(
        !text(&up.stdout).contains("HOOK-OUTPUT"),
        "hook output never reaches stdout"
    );
    assert!(text(&up.stderr).contains("HOOK-OUTPUT"));
    assert!(text(&up.stderr).contains("Framework booted"));
}

#[test]
fn a_fatal_framework_boot_fails_up_with_125_one_failure_document_and_the_fatal_line() {
    let fixture = Fixture::new("fatal");
    let started = std::time::Instant::now();
    // No `--wait-timeout`: a PHP fatal is a real error, so it fails at once.
    let up = fixture.berth(&["up", "--json"], &[("FAKE_BOOT", "fatal")]);
    assert!(started.elapsed().as_secs() < 20, "a fatal boot never waits");
    assert_eq!(
        up.status.code(),
        Some(125),
        "a berth failure, not the probe's 255"
    );
    let document = one_document(&up.stdout);
    assert_eq!(document["status"], "failed");
    let error = document["error"].as_str().expect("error");
    assert!(
        error.contains("artisan about") && error.contains("bootstrap/app.php"),
        "{error}"
    );
    assert!(
        text(&up.stderr).contains("bootstrap/app.php"),
        "{}",
        text(&up.stderr)
    );

    // Readiness is not liveness: a second `up` keeps the running app.
    let log = fixture.base.join("state/log");
    std::fs::remove_file(&log).expect("clear log");
    let again = fixture.berth(&["up", "--json"], &[("FAKE_BOOT", "fatal")]);
    assert_eq!(again.status.code(), Some(125));
    let calls = std::fs::read_to_string(&log).expect("log");
    assert!(
        !calls
            .lines()
            .any(|call| (call.starts_with("rm ") || call.starts_with("run "))
                && call.contains("_app")),
        "the app container is never replaced for failing readiness: {calls}"
    );
}

#[test]
fn an_app_that_stops_while_booting_fails_at_once() {
    let fixture = Fixture::new("exited");
    let started = std::time::Instant::now();
    // No `--wait-timeout`: a stopped container ends the wait by itself.
    let up = fixture.berth(&["up"], &[("FAKE_BOOT", "exit")]);
    assert_eq!(up.status.code(), Some(125));
    assert!(
        started.elapsed().as_secs() < 20,
        "a stopped app never waits"
    );
    assert!(
        text(&up.stderr).contains("stopped while booting"),
        "{}",
        text(&up.stderr)
    );
}

#[test]
fn a_probe_the_engine_cannot_run_waits_and_fails_only_at_an_opt_in_cap() {
    let fixture = Fixture::new("capped");
    let started = std::time::Instant::now();
    let up = fixture.berth(
        &["up", "--json", "--wait-timeout", "2"],
        &[("FAKE_BOOT", "restarting")],
    );
    assert_eq!(up.status.code(), Some(125), "{}", text(&up.stderr));
    assert!(started.elapsed().as_secs() < 20, "the cap ends the wait");
    let error = one_document(&up.stdout)["error"]
        .as_str()
        .expect("error")
        .to_owned();
    assert!(error.contains("--wait-timeout"), "{error}");
    assert!(
        error.contains("the last probe said") && error.contains("is restarting"),
        "the capped wait says what it waited on: {error}"
    );
}

#[test]
fn an_engine_that_fails_to_run_the_probe_is_asked_again() {
    let fixture = Fixture::new("oci");
    let up = fixture.berth(&["up", "--json"], &[("FAKE_BOOT", "oci-then-ok")]);
    assert!(up.status.success(), "{}", text(&up.stderr));
    assert_eq!(one_document(&up.stdout)["status"], "ready");
}

const HORIZON_PROJECT: &str = "schema_version: 9\nproject: shop\nproject_type: project\nservices:\n  cache:\n    preset: redis\n    isolation: dedicated\n  horizon:\n    preset: horizon\n";

#[test]
fn a_broken_horizon_fails_at_once_and_a_starting_one_is_waited_out() {
    let fixture = Fixture::new("horizon");
    fixture.write_project(HORIZON_PROJECT);
    let started = std::time::Instant::now();
    let broken = fixture.berth(&["up", "--json"], &[("FAKE_HORIZON", "broken")]);
    assert_eq!(broken.status.code(), Some(125), "{}", text(&broken.stderr));
    assert!(
        started.elapsed().as_secs() < 20,
        "a broken worker never waits"
    );
    let error = one_document(&broken.stdout)["error"]
        .as_str()
        .expect("error")
        .to_owned();
    assert!(error.contains("config/horizon.php"), "{error}");

    let starting = Fixture::new("horizon-starting");
    starting.write_project(HORIZON_PROJECT);
    let up = starting.berth(&["up", "--json"], &[("FAKE_HORIZON", "inactive-twice")]);
    assert!(up.status.success(), "{}", text(&up.stderr));
}

#[test]
fn a_hanging_probe_is_ended_by_an_opt_in_cap() {
    let fixture = Fixture::new("hang");
    let started = std::time::Instant::now();
    let up = fixture.berth(
        &["up", "--json", "--wait-timeout", "2"],
        &[("FAKE_BOOT", "hang")],
    );
    assert_eq!(up.status.code(), Some(125), "{}", text(&up.stderr));
    assert!(
        started.elapsed().as_secs() < 20,
        "the cap kills the probe in flight"
    );
}

#[test]
fn a_slow_start_without_a_cap_waits_until_the_app_boots() {
    let fixture = Fixture::new("slow");
    let up = fixture.berth(&["up", "--json"], &[("FAKE_BOOT", "restarting-twice")]);
    assert!(up.status.success(), "{}", text(&up.stderr));
    assert_eq!(one_document(&up.stdout)["status"], "ready");
}

#[test]
fn a_boot_exception_laravel_renders_fails_at_once_without_a_cap() {
    let fixture = Fixture::new("rendered");
    let started = std::time::Instant::now();
    let up = fixture.berth(&["up", "--json"], &[("FAKE_BOOT", "rendered")]);
    assert_eq!(up.status.code(), Some(125), "{}", text(&up.stderr));
    assert!(started.elapsed().as_secs() < 20, "a broken app never waits");
    let error = one_document(&up.stdout)["error"]
        .as_str()
        .expect("error")
        .to_owned();
    assert!(error.contains("routes/console.php"), "{error}");
}

#[test]
fn a_database_that_stops_while_starting_fails_up_at_once_without_a_cap() {
    let fixture = Fixture::new("db-exits");
    let started = std::time::Instant::now();
    let up = fixture.berth(&["up", "--json"], &[("FAKE_DB_HEALTH", "exit")]);
    assert_eq!(up.status.code(), Some(125), "{}", text(&up.stderr));
    assert!(
        started.elapsed().as_secs() < 20,
        "a stopped database never waits"
    );
    let error = one_document(&up.stdout)["error"]
        .as_str()
        .expect("error")
        .to_owned();
    assert!(error.contains("is not running"), "{error}");
}

#[test]
fn one_services_failure_ends_a_parallel_siblings_wait_and_is_the_error_reported() {
    let fixture = Fixture::new("siblings");
    fixture.write_project(
        "schema_version: 9\nproject: shop\nproject_type: project\nservices:\n  db:\n    preset: postgres\n    isolation: dedicated\n  cache:\n    preset: redis\n    isolation: dedicated\n",
    );
    let started = std::time::Instant::now();
    let up = fixture.berth(
        &["up", "--json"],
        &[("FAKE_DB_HEALTH", "exit"), ("FAKE_CACHE_HEALTH", "never")],
    );
    assert_eq!(up.status.code(), Some(125), "{}", text(&up.stderr));
    assert!(
        started.elapsed().as_secs() < 20,
        "the cache's endless wait ends once the database failed"
    );
    let error = one_document(&up.stdout)["error"]
        .as_str()
        .expect("error")
        .to_owned();
    assert!(
        error.contains("is not running") && error.contains("_db"),
        "{error}"
    );
    assert!(!error.contains("another service failed"), "{error}");
}

#[test]
fn up_json_without_waiting_reports_unchecked_health_and_refuses_dry_run() {
    let fixture = Fixture::new("nowait");
    let up = fixture.berth(&["up", "--json", "--no-wait"], &[]);
    assert!(up.status.success(), "{}", text(&up.stderr));
    let document = one_document(&up.stdout);
    for service in document["services"].as_array().expect("services") {
        assert_eq!(service["health"], "not_checked", "{service}");
    }
    assert!(
        !fixture
            .berth(&["up", "--json", "--no-wait"], &[])
            .stdout
            .is_empty(),
        "a converged instance still reports"
    );
    let dry = fixture.berth(&["--dry-run", "up", "--json"], &[]);
    assert_eq!(dry.status.code(), Some(125));
    assert!(dry.stdout.is_empty());
}

#[test]
fn credentials_never_reach_stderr_or_logs_even_when_docker_fails() {
    let fixture = Fixture::new("secrets");
    let up = fixture.berth(&["up", "--json"], &[]);
    assert!(up.status.success(), "{}", text(&up.stderr));
    assert!(!text(&up.stderr).contains("canary-s3cret-value"));

    let failing = Fixture::new("secrets-fail");
    let failed = failing.berth(&["up", "--json"], &[("FAKE_DB_RUN", "busy")]);
    assert_eq!(failed.status.code(), Some(125));
    let stderr = text(&failed.stderr);
    assert!(!stderr.contains("canary-s3cret-value"), "{stderr}");
    assert!(
        !text(&failed.stdout).contains("canary-s3cret-value"),
        "the failure document has no credentials"
    );

    let dry = failing.berth(&["--dry-run", "up"], &[]);
    assert!(
        !text(&dry.stderr).contains("canary-s3cret-value"),
        "{}",
        text(&dry.stderr)
    );
    for logs in [fixture.logs(), failing.logs()] {
        assert!(!logs.contains("canary-s3cret-value"), "{logs}");
    }
}

#[test]
fn a_cold_start_pull_never_writes_ahead_of_the_document() {
    let fixture = Fixture::new("cold");
    let up = fixture.berth(&["up", "--json"], &[("FAKE_COLD", "1")]);
    assert!(up.status.success(), "{}", text(&up.stderr));
    assert!(
        fixture.base.join("state/pulled").exists(),
        "the image was pulled"
    );
    let document = one_document(&up.stdout);
    assert_eq!(document["status"], "ready");
    assert!(
        text(&up.stderr).contains("Pulling from"),
        "pull output goes to stderr"
    );
}

#[test]
fn seed_restores_never_write_ahead_of_the_document() {
    let fixture = Fixture::new("seed");
    std::fs::write(fixture.base.join("project/seed.sql"), "SELECT 1;\n").expect("seed");
    fixture.write_project(&PROJECT.replace(
        "    preset: postgres\n    isolation: dedicated\n",
        "    preset: postgres\n    isolation: dedicated\n    seed_file: seed.sql\n",
    ));
    let up = fixture.berth(&["up", "--json", "--seed"], &[]);
    assert!(up.status.success(), "{}", text(&up.stderr));
    let document = one_document(&up.stdout);
    assert_eq!(document["status"], "ready");
}

#[test]
fn several_app_services_none_named_app_still_give_one_ready_document() {
    let fixture = Fixture::new("apps");
    fixture.write_project(
        "schema_version: 9\nproject: shop\nproject_type: project\nservices:\n  db:\n    preset: postgres\n    isolation: dedicated\n  web:\n    preset: laravel\n  worker:\n    preset: queue-worker\n",
    );
    let up = fixture.berth(&["up", "--json"], &[]);
    assert!(up.status.success(), "{}", text(&up.stderr));
    let document = one_document(&up.stdout);
    assert_eq!(document["status"], "ready");
    assert_eq!(document["environment"]["DB_HOST"], "db");
    let worker = document["services"]
        .as_array()
        .expect("services")
        .iter()
        .find(|svc| svc["name"] == "worker")
        .expect("worker");
    assert_eq!(
        worker["health"], "unchecked",
        "a worker has no readiness probe"
    );
}

#[test]
fn services_outside_the_selection_are_never_reported_healthy() {
    let fixture = Fixture::new("subset");
    let up = fixture.berth(&["up", "--json", "--service", "app"], &[]);
    assert!(up.status.success(), "{}", text(&up.stderr));
    let document = one_document(&up.stdout);
    let db = document["services"]
        .as_array()
        .expect("services")
        .iter()
        .find(|svc| svc["name"] == "db")
        .expect("db");
    assert_ne!(db["health"], "healthy", "{db}");
}

#[test]
fn a_laravel_without_about_boots_through_artisan_version() {
    let fixture = Fixture::new("old-laravel");
    let up = fixture.berth(
        &["up", "--json", "--wait-timeout", "10"],
        &[("FAKE_BOOT", "undefined")],
    );
    assert!(up.status.success(), "{}", text(&up.stderr));
    assert_eq!(one_document(&up.stdout)["status"], "ready");
}

#[test]
fn default_credentials_never_mangle_names_routes_or_errors() {
    let fixture = Fixture::new("laravel-named");
    fixture.write_project(
        "schema_version: 9\nproject: laravel-shop\nproject_type: project\nservices:\n  db:\n    preset: postgres\n    isolation: dedicated\n  app:\n    preset: laravel\n",
    );
    let up = fixture.berth(
        &["up", "--json", "--wait-timeout", "5"],
        &[("FAKE_BOOT", "fatal")],
    );
    assert_eq!(up.status.code(), Some(125));
    let stderr = text(&up.stderr);
    assert!(!stderr.contains("<redacted>-shop"), "{stderr}");
    assert!(stderr.contains("laravel-shop"), "{stderr}");
    let document = one_document(&up.stdout);
    assert_eq!(document["project"], "laravel-shop");
    assert!(
        !document["error"]
            .as_str()
            .unwrap_or_default()
            .contains("<redacted>")
    );
}

#[test]
fn inspection_commands_print_exactly_one_json_document() {
    let fixture = Fixture::new("inspect-json");
    assert!(
        fixture
            .berth(&["up", "--json", "--no-wait"], &[])
            .status
            .success()
    );
    for (args, succeeds) in [
        (&["ps", "--json"][..], true),
        (&["url", "--json"][..], true),
        (&["about", "--json"][..], true),
        (&["ls", "--json"][..], true),
        // Health and doctor fail against the fake gateway; their document
        // reports it.
        (&["health", "--json"][..], false),
        (&["doctor", "--json"][..], false),
    ] {
        let output = fixture.berth(args, &[]);
        if succeeds {
            assert!(
                output.status.success(),
                "{args:?}: {}",
                text(&output.stderr)
            );
        }
        let document = one_document(&output.stdout);
        // The documented shapes (docs/v9/agents.md), not just "some JSON".
        let keys: &[&str] = match args[0] {
            "ps" => &["name", "kind", "driver", "image", "port", "status"],
            "url" => &["name", "url", "published"],
            "health" => &["name", "ok", "error"],
            "about" => &["application", "environment", "services"],
            "ls" => &["instances"],
            _ => &["ok", "error", "checks"],
        };
        let sample = if document.is_array() {
            assert!(
                document.as_array().is_some_and(|rows| !rows.is_empty()),
                "{args:?}: {document}"
            );
            document[0].clone()
        } else {
            document.clone()
        };
        for key in keys {
            assert!(
                sample.get(*key).is_some(),
                "{args:?} lacks {key}: {document}"
            );
        }
    }
    let doctor = one_document(&fixture.berth(&["doctor", "--json"], &[]).stdout);
    assert!(
        doctor["checks"]
            .as_array()
            .is_some_and(|checks| checks.iter().any(|check| check["status"] == "ok")),
        "doctor --json lists its checks: {doctor}"
    );
}

/// Every stdout line of `logs --json` as an object.
fn log_lines(output: &Output) -> Vec<serde_json::Value> {
    assert!(output.status.success(), "{}", text(&output.stderr));
    text(&output.stdout)
        .lines()
        .map(|line| serde_json::from_str(line).expect("one JSON object per line"))
        .collect()
}

#[test]
fn logs_json_prints_one_object_per_line_for_every_selection() {
    let fixture = Fixture::new("logs-json");
    assert!(
        fixture
            .berth(&["up", "--json", "--no-wait"], &[])
            .status
            .success()
    );
    let single = log_lines(&fixture.berth(&["logs", "--json", "--service", "db"], &[]));
    assert!(
        single.iter().any(|line| line["service"] == "db"
            && line["stream"] == "stdout"
            && line["line"] == "ready to accept connections"),
        "{single:?}"
    );
    assert!(
        single.iter().any(|line| line["stream"] == "stderr"),
        "{single:?}"
    );
    assert!(
        single.iter().any(|line| line["line"] == "after"),
        "a line that is not UTF-8 never ends the stream: {single:?}"
    );
    assert!(
        single.iter().any(|line| line["line"] == "\u{fffd}bad"),
        "{single:?}"
    );
    for args in [
        &["logs", "--json"][..],
        &[
            "logs",
            "--json",
            "--follow",
            "--service",
            "db",
            "--service",
            "app",
        ][..],
    ] {
        let lines = log_lines(&fixture.berth(args, &[]));
        let services: std::collections::BTreeSet<&str> = lines
            .iter()
            .filter_map(|line| line["service"].as_str())
            .collect();
        assert!(services.len() > 1, "{args:?}: {lines:?}");
    }
    let access = fixture.berth(&["logs", "--json", "--access"], &[]);
    assert_eq!(access.status.code(), Some(125));
    assert!(
        text(&access.stderr).contains("not supported"),
        "{}",
        text(&access.stderr)
    );
    let dry = fixture.berth(&["--dry-run", "logs", "--json"], &[]);
    assert_eq!(dry.status.code(), Some(125));
    assert!(dry.stdout.is_empty());
}

#[test]
fn logs_all_skips_services_without_a_container_but_a_named_one_stays_strict() {
    let fixture = Fixture::new("logs-all");
    assert!(
        fixture
            .berth(&["up", "--json", "--no-wait"], &[])
            .status
            .success()
    );
    let app = std::fs::read_dir(fixture.base.join("state/c"))
        .expect("containers")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|path| path.to_string_lossy().ends_with("_app"))
        .expect("app container");
    std::fs::remove_file(&app).expect("remove the app container");

    let all = fixture.berth(&["logs", "--all", "--json"], &[]);
    assert!(
        all.status.success(),
        "{}",
        String::from_utf8_lossy(&all.stderr)
    );
    let lines = log_lines(&all);
    assert!(
        lines.iter().any(|line| line["service"] == "db"),
        "{lines:?}"
    );
    assert!(
        !lines.iter().any(|line| line["service"] == "app"),
        "{lines:?}"
    );

    let named = fixture.berth(&["logs", "--service", "app"], &[]);
    assert!(
        !named.status.success(),
        "a named service without a container fails"
    );
}

#[test]
fn parallel_ups_build_a_missing_derived_image_once() {
    // Thirty agents on a cold cache would otherwise all compile the same PHP
    // extensions at once (a 2 GiB Podman VM OOM-killed the second build).
    let fixture = Fixture::new("derived");
    let env = [("FAKE_DERIVED", "1")];
    let outputs = std::thread::scope(|scope| {
        let runs: Vec<_> = ["a", "b"]
            .into_iter()
            .map(|instance| {
                let fixture = &fixture;
                scope.spawn(move || {
                    fixture.berth(&["up", "--no-wait", "--instance", instance], &env)
                })
            })
            .collect();
        runs.into_iter()
            .map(|run| run.join().expect("berth thread"))
            .collect::<Vec<_>>()
    });
    for output in &outputs {
        assert!(output.status.success(), "{}", text(&output.stderr));
    }
    let builds = std::fs::read_to_string(fixture.base.join("state/builds")).unwrap_or_default();
    assert_eq!(builds.lines().count(), 1, "{builds}");
}

/// Runs `command` to completion, failing the test instead of hanging when a
/// regression makes berth wait forever (berth itself has no deadlines).
fn bounded_output(command: &mut Command) -> Output {
    use std::io::Read;
    let mut child = command
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("run berth");
    fn drain<R: Read + Send + 'static>(pipe: Option<R>) -> std::thread::JoinHandle<Vec<u8>> {
        std::thread::spawn(move || {
            let mut bytes = Vec::new();
            if let Some(mut pipe) = pipe {
                drop(pipe.read_to_end(&mut bytes));
            }
            bytes
        })
    }
    let stdout = drain(child.stdout.take());
    let stderr = drain(child.stderr.take());
    let started = std::time::Instant::now();
    let mut timed_out = false;
    let status = loop {
        if let Some(status) = child.try_wait().expect("wait for berth") {
            break status;
        }
        if started.elapsed() > std::time::Duration::from_secs(120) {
            timed_out = true;
            drop(child.kill());
            break child.wait().expect("wait for the killed berth");
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    };
    assert!(
        !timed_out,
        "berth did not finish within 120s: a wait that should end did not"
    );
    Output {
        status,
        stdout: stdout.join().expect("stdout"),
        stderr: stderr.join().expect("stderr"),
    }
}
