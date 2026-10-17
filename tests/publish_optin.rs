//! Host-port publication across separate `berth` commands, with a fake
//! `docker` that keeps each container's port binding.

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

/// `state/c/<name>` exists per container; `state/p/<name>` holds the host
/// port of a published one.
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
  "image inspect") exit 0 ;;
esac
case "$1" in
  run)
    name=""; prev=""; port=""
    for arg in "$@"; do
      [ "$prev" = "--name" ] && name="$arg"
      [ "$prev" = "-p" ] && port=$(echo "$arg" | cut -d: -f2)
      prev="$arg"
    done
    case "$*" in *"caddy run"*) [ -n "$FAKE_GATEWAY_FAIL" ] && { echo "port is already allocated" >&2; exit 125; } ;; esac
    mkdir -p "$S/c" "$S/p"; touch "$S/c/$name"
    [ -n "$port" ] && echo "$port" > "$S/p/$name"
    exit 0 ;;
  rm) for arg in "$@"; do rm -f "$S/c/$arg" "$S/p/$arg" "$S/stopped-$arg"; done; exit 0 ;;
  stop) for arg in "$@"; do [ -f "$S/c/$arg" ] && touch "$S/stopped-$arg"; done; exit 0 ;;
  start) for arg in "$@"; do rm -f "$S/stopped-$arg"; done; exit 0 ;;
  inspect)
    name=""; for arg in "$@"; do name="$arg"; done
    [ -f "$S/c/$name" ] || exit 1
    case "$*" in
      *--format*NetworkSettings.Networks*) printf '{}' ;;
      *--format*) if [ -f "$S/stopped-$name" ]; then printf 'exited'; else printf 'running'; fi ;;
      *)
        status=running; [ -f "$S/stopped-$name" ] && status=exited
        if [ -f "$S/p/$name" ]; then
          binding=$(printf '{"11211/tcp":[{"HostIp":"127.0.0.1","HostPort":"%s"}]}' "$(cat "$S/p/$name")")
          live="$binding"; [ "$status" = exited ] && live='{}'
          printf '[{"State":{"Status":"%s"},"NetworkSettings":{"Ports":%s},"HostConfig":{"PortBindings":%s}}]' "$status" "$live" "$binding"
        else
          printf '[{"State":{"Status":"%s"},"NetworkSettings":{"Ports":{}},"HostConfig":{"PortBindings":{}}}]' "$status"
        fi ;;
    esac
    exit 0 ;;
  exec)
    case "$*" in
      *wget*) printf '{"status":"up"}'; echo "  HTTP/1.1 200 OK" >&2 ;;
    esac
    exit 0 ;;
esac
exit 0
"#;

struct Fixture {
    base: PathBuf,
}

impl Fixture {
    fn new(label: &str, services: &str) -> Self {
        let base =
            std::env::temp_dir().join(format!("berth-publish-{label}-{}", std::process::id()));
        drop(std::fs::remove_dir_all(&base));
        for dir in ["project", "bin", "state/c", "home"] {
            std::fs::create_dir_all(base.join(dir)).expect("fixture dir");
        }
        std::fs::write(
            base.join("project/.berth.yaml"),
            format!(
                "schema_version: 9\nproject: api\nproject_type: project\nservices:\n{services}"
            ),
        )
        .expect("project file");
        let docker = base.join("bin/docker");
        std::fs::write(&docker, FAKE_DOCKER).expect("fake docker");
        std::fs::set_permissions(&docker, std::fs::Permissions::from_mode(0o755)).expect("chmod");
        Self { base }
    }

    fn berth(&self, args: &[&str], gateway_fails: bool) -> Output {
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
            .env("BERTH_HOME", self.base.join("home"))
            .env("FAKE_STATE", self.base.join("state"))
            .env_remove("BERTH_INSTANCE")
            .env_remove("BERTH_GATEWAY_HTTPS_PORT")
            .env_remove("BERTH_GATEWAY_HTTP_PORT");
        if gateway_fails {
            command.env("FAKE_GATEWAY_FAIL", "1");
        }
        command.output().expect("run berth")
    }

    fn calls(&self) -> Vec<String> {
        std::fs::read_to_string(self.base.join("state/log"))
            .unwrap_or_default()
            .lines()
            .map(ToOwned::to_owned)
            .collect()
    }

    fn clear_log(&self) {
        drop(std::fs::remove_file(self.base.join("state/log")));
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

const CACHE: &str = "  cache:\n    preset: memcached\n";

#[test]
fn a_port_published_by_up_p_is_seen_by_later_commands_and_kept_by_recreate() {
    let fixture = Fixture::new("later", CACHE);
    let up = fixture.berth(&["up", "--no-wait", "-P"], false);
    assert!(up.status.success(), "{}", text(&up.stderr));

    let url = fixture.berth(&["url", "--service", "cache", "--format", "json"], false);
    assert!(url.status.success(), "{}", text(&url.stderr));
    assert!(
        text(&url.stdout).contains("\"published\": true"),
        "{}",
        text(&url.stdout)
    );

    let bound = std::fs::read_dir(fixture.base.join("state/p"))
        .expect("bindings")
        .filter_map(Result::ok)
        .map(|entry| {
            std::fs::read_to_string(entry.path())
                .expect("port")
                .trim()
                .to_owned()
        })
        .next()
        .expect("bound port");
    fixture.clear_log();
    let recreate = fixture.berth(&["recreate", "--no-wait", "--service", "cache"], false);
    assert!(recreate.status.success(), "{}", text(&recreate.stderr));
    let run = fixture
        .calls()
        .into_iter()
        .find(|call| call.starts_with("run "))
        .expect("recreated");
    assert!(
        run.contains(&format!(" -p 127.0.0.1:{bound}:")),
        "recreate keeps the port `up -P` assigned ({bound}): {run}"
    );
}

#[test]
fn recreate_p_publishes() {
    let fixture = Fixture::new("recreate-p", CACHE);
    assert!(fixture.berth(&["up", "--no-wait"], false).status.success());
    fixture.clear_log();
    let recreate = fixture.berth(
        &["recreate", "-P", "--no-wait", "--service", "cache"],
        false,
    );
    assert!(recreate.status.success(), "{}", text(&recreate.stderr));
    let run = fixture
        .calls()
        .into_iter()
        .find(|call| call.starts_with("run "))
        .expect("recreated");
    assert!(run.contains(" -p 127.0.0.1:"), "{run}");
}

#[test]
fn unpublished_http_services_without_routes_are_probed_inside_their_network() {
    let fixture = Fixture::new("gotenberg", "  pdf:\n    preset: gotenberg\n");
    let up = fixture.berth(&["up", "--wait-timeout", "10"], false);
    assert!(up.status.success(), "{}", text(&up.stderr));
    assert!(
        fixture
            .calls()
            .iter()
            .any(|call| call.starts_with("exec berth_gateway_") && call.contains("wget")),
        "{:#?}",
        fixture.calls()
    );
}

#[test]
fn a_gateway_that_cannot_start_fails_the_health_wait_at_once_with_its_cause() {
    let fixture = Fixture::new("gateway-fail", CACHE);
    let started = std::time::Instant::now();
    let up = fixture.berth(&["up", "--wait-timeout", "60"], true);
    assert!(!up.status.success());
    assert!(started.elapsed().as_secs() < 30, "failed fast");
    let stderr = text(&up.stderr);
    assert!(
        stderr.contains("cannot probe 'cache'") && stderr.contains("allocated"),
        "{stderr}"
    );
}

#[test]
fn health_never_starts_the_gateway() {
    let fixture = Fixture::new("health", CACHE);
    assert!(fixture.berth(&["up", "--no-wait"], false).status.success());
    fixture.clear_log();
    let health = fixture.berth(&["health", "--timeout", "2"], false);
    assert!(!health.status.success(), "unknown health is not healthy");
    assert!(
        !fixture
            .calls()
            .iter()
            .any(|call| call.starts_with("run ") && call.contains("caddy")),
        "{:#?}",
        fixture.calls()
    );
}

#[test]
fn up_p_after_a_plain_up_recreates_the_container_with_a_host_port() {
    let fixture = Fixture::new("drift", CACHE);
    assert!(fixture.berth(&["up", "--no-wait"], false).status.success());
    fixture.clear_log();
    let up = fixture.berth(&["up", "--no-wait", "-P"], false);
    assert!(up.status.success(), "{}", text(&up.stderr));
    let run = fixture
        .calls()
        .into_iter()
        .find(|call| call.starts_with("run "))
        .expect("the unpublished container is replaced");
    assert!(run.contains(" -p 127.0.0.1:"), "{run}");
}

#[test]
fn a_stopped_published_container_is_started_again_not_recreated() {
    let fixture = Fixture::new("stopped", CACHE);
    assert!(
        fixture
            .berth(&["up", "--no-wait", "-P"], false)
            .status
            .success()
    );
    assert!(fixture.berth(&["stop"], false).status.success());
    fixture.clear_log();
    let up = fixture.berth(&["up", "--no-wait", "-P"], false);
    assert!(up.status.success(), "{}", text(&up.stderr));
    let calls = fixture.calls();
    assert!(
        calls.iter().any(|call| call.starts_with("start ")),
        "{calls:#?}"
    );
    assert!(
        !calls
            .iter()
            .any(|call| call.starts_with("run ") && call.contains("_cache")),
        "the stopped container is reused: {calls:#?}"
    );
}
