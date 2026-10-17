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

/// `FAKE_BOOT=ok|fatal|exit`: how `php artisan about` answers (`exit` stops
/// the app container). `FAKE_DB_RUN=busy` makes the database's `run` fail.
const FAKE_DOCKER: &str = r#"#!/bin/sh
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
  "image inspect") [ -n "$FAKE_COLD" ] && [ ! -f "$S/pulled" ] && exit 1; exit 0 ;;
esac
case "$1" in
  pull) echo "17-alpine: Pulling from library/postgres"; echo "Status: Downloaded newer image"; touch "$S/pulled"; exit 0 ;;
  run)
    name=""; prev=""
    for arg in "$@"; do [ "$prev" = "--name" ] && name="$arg"; prev="$arg"; done
    case "$name" in *_db) [ "$FAKE_DB_RUN" = busy ] && { echo "port is already allocated" >&2; exit 125; } ;; esac
    mkdir -p "$S/c"; echo running > "$S/c/$name"; exit 0 ;;
  rm) for arg in "$@"; do rm -f "$S/c/$arg"; done; exit 0 ;;
  inspect)
    name=""; for arg in "$@"; do name="$arg"; done
    [ -f "$S/c/$name" ] || exit 1
    case "$*" in
      *NetworkSettings.Networks*) printf '{}' ;;
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
        esac
        echo "Environment ... local"; exit 0 ;;
      *wget*) printf 'ok'; echo "  HTTP/1.1 200 OK" >&2 ;;
      *" -i "*) cat >/dev/null; echo "SET"; echo "CREATE TABLE" ;;
    esac
    exit 0 ;;
esac
exit 0
"#;

const FAKE_CURL: &str = "#!/bin/sh\nprintf 'ok\\n200'\n";

const PROJECT: &str = "schema_version: 9\nproject: shop\nproject_type: project\nservices:\n  db:\n    preset: postgres\n    isolation: dedicated\n    password: canary-s3cret-value\n    env:\n      DATABASE_URL: postgres://u:canary-s3cret-value@db/x\n  app:\n    preset: laravel\n    hook:\n      - name: greet\n        phase: post_up\n        run:\n          type: script\n          path: hook.sh\n";

struct Fixture {
    base: PathBuf,
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
        Self { base }
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
            .env("FAKE_STATE", self.base.join("state"))
            .env_remove("BERTH_INSTANCE")
            .env_remove("BERTH_GATEWAY_HTTPS_PORT")
            .env_remove("BERTH_GATEWAY_HTTP_PORT");
        for pair in env {
            command.env(pair.0, pair.1);
        }
        command.output().expect("run berth")
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
            .is_some_and(|text| text.ends_with(".berth.localhost:8443"))
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
    let up = fixture.berth(
        &["up", "--json", "--wait-timeout", "5"],
        &[("FAKE_BOOT", "fatal")],
    );
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
    let again = fixture.berth(
        &["up", "--json", "--wait-timeout", "5"],
        &[("FAKE_BOOT", "fatal")],
    );
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
    let up = fixture.berth(&["up", "--wait-timeout", "60"], &[("FAKE_BOOT", "exit")]);
    assert_eq!(up.status.code(), Some(125));
    assert!(
        started.elapsed().as_secs() < 20,
        "no waiting out the timeout"
    );
    assert!(
        text(&up.stderr).contains("stopped while booting"),
        "{}",
        text(&up.stderr)
    );
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
