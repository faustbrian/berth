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

struct Fixture {
    base: PathBuf,
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
        Self { base }
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
