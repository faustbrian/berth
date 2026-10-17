//! Instance-network lifecycle through the real `berth` binary, with a fake
//! `docker` first on PATH that records every call.

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

const FAKE_DOCKER: &str = r#"#!/bin/sh
echo "$*" >> "$FAKE_STATE/log"
case "$1 $2" in
  "network inspect")
    [ -f "$FAKE_STATE/network" ] || exit 1
    case "$*" in *--format*) cat "$BERTH_HOME/installation-id" ;; esac
    exit 0 ;;
  "network create") touch "$FAKE_STATE/network"; exit 0 ;;
  "network rm") rm -f "$FAKE_STATE/network"; exit 0 ;;
  "ps -a") cat "$FAKE_STATE/attached" 2>/dev/null; exit 0 ;;
esac
case "$1" in
  inspect) [ -f "$FAKE_STATE/container" ] || exit 1; printf running; exit 0 ;;
  run) touch "$FAKE_STATE/container"; exit 0 ;;
  rm) rm -f "$FAKE_STATE/container"; exit 0 ;;
esac
exit 0
"#;

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
        if claimed && std::net::TcpListener::bind(("127.0.0.1", port)).is_ok() {
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
        let base = std::env::temp_dir().join(format!("berth-net-{label}-{}", std::process::id()));
        drop(std::fs::remove_dir_all(&base));
        for dir in ["project", "bin", "state", "home"] {
            std::fs::create_dir_all(base.join(dir)).expect("fixture dir");
        }
        std::fs::write(
            base.join("project/.berth.yaml"),
            "schema_version: 9\nproject: api\nproject_type: project\nservices:\n  cache:\n    preset: redis\n",
        )
        .expect("project file");
        let docker = base.join("bin/docker");
        std::fs::write(&docker, FAKE_DOCKER).expect("fake docker");
        std::fs::set_permissions(&docker, std::fs::Permissions::from_mode(0o755)).expect("chmod");
        Self {
            base,
            ports: free_ports(),
        }
    }

    fn berth(&self, args: &[&str]) -> Output {
        let path = format!(
            "{}:{}",
            self.base.join("bin").display(),
            std::env::var("PATH").unwrap_or_default()
        );
        Command::new(env!("CARGO_BIN_EXE_berth"))
            .current_dir(self.base.join("project"))
            .args(args)
            .env("PATH", path)
            .env("BERTH_HOME", self.base.join("home"))
            .env("BERTH_GATEWAY_HTTPS_PORT", self.ports.0.to_string())
            .env("BERTH_GATEWAY_HTTP_PORT", self.ports.1.to_string())
            .env("FAKE_STATE", self.base.join("state"))
            .env_remove("BERTH_INSTANCE")
            .output()
            .expect("run berth")
    }

    fn calls(&self) -> Vec<String> {
        std::fs::read_to_string(self.base.join("state/log"))
            .unwrap_or_default()
            .lines()
            .map(ToOwned::to_owned)
            .collect()
    }

    fn position(&self, prefix: &str) -> Option<usize> {
        self.calls()
            .iter()
            .position(|call| call.starts_with(prefix))
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        release_ports(self.ports);
        drop(std::fs::remove_dir_all(&self.base));
    }
}

#[test]
fn commands_that_start_containers_create_the_network_first_from_a_cold_start() {
    for args in [&["up"][..], &["recreate", "--service", "cache"][..]] {
        let fixture = Fixture::new(&format!("cold-{}", args[0]));
        let output = fixture.berth(args);
        let create = fixture.position("network create");
        let run = fixture.position("run ");
        assert!(
            create.is_some() && run.is_some() && create < run,
            "{args:?}: {:?}\nstderr: {}",
            fixture.calls(),
            String::from_utf8_lossy(&output.stderr)
        );
        let run_call = &fixture.calls()[run.expect("run")];
        assert!(run_call.contains("--network berth_api_"), "{run_call}");
        assert!(run_call.contains("--network-alias cache"), "{run_call}");
    }
}

#[test]
fn down_keeps_a_network_that_a_stopped_container_still_references() {
    let fixture = Fixture::new("stopped");
    assert!(fixture.berth(&["up"]).status.success());
    std::fs::write(fixture.base.join("state/attached"), "abc123\n").expect("attached");
    assert!(fixture.berth(&["down"]).status.success());
    assert!(
        fixture.position("network rm").is_none(),
        "{:?}",
        fixture.calls()
    );

    std::fs::remove_file(fixture.base.join("state/attached")).expect("detach");
    assert!(fixture.berth(&["down"]).status.success());
    assert!(
        fixture.position("network rm").is_some(),
        "{:?}",
        fixture.calls()
    );
}

#[test]
fn services_publish_host_ports_only_when_opted_in() {
    let first_run = |label: &str, args: &[&str]| {
        let fixture = Fixture::new(label);
        assert!(fixture.berth(args).status.success(), "{args:?}");
        fixture
            .calls()
            .into_iter()
            .find(|call| call.starts_with("run "))
            .expect("run")
    };
    let default = first_run("publish", &["up", "--no-wait"]);
    assert!(
        !default.contains(" -p "),
        "no host port by default: {default}"
    );
    let publish_all = first_run("publish-all", &["up", "--no-wait", "-P"]);
    assert!(
        publish_all.contains(" -p 127.0.0.1:"),
        "-P publishes like 7.0.0: {publish_all}"
    );
    let env = first_run("publish-env", &["--env", "staging", "up", "--no-wait"]);
    assert!(
        env.contains(" -p 127.0.0.1:"),
        "--env namespaces keep offset host ports: {env}"
    );
}
