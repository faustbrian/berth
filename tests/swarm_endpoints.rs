//! Swarm `inject_env` endpoints through the real `berth` binary with a fake
//! `docker` (ledger L205): a consumer receives the source app container's
//! in-network address and joins the source instance's network.

// Integration tests are their own crate: panicking on setup failures and
// top-level `#[test]` functions are the idiom here.
#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::tests_outside_test_module
)]

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// Networks are `n-{name}` files; `network connect` records `att-{network}`
/// lines naming the container, and fails for a network that does not exist.
const FAKE_DOCKER: &str = r#"#!/bin/sh
S="$FAKE_STATE"
echo "$*" >> "$S/log"
case "$1 $2" in
  "network inspect")
    net=""; for arg in "$@"; do net="$arg"; done
    [ -f "$S/n-$net" ] || exit 1
    case "$*" in *--format*) cat "$BERTH_HOME/installation-id" ;; esac
    exit 0 ;;
  "network create") net=""; for arg in "$@"; do net="$arg"; done; touch "$S/n-$net"; exit 0 ;;
  "network rm") rm -f "$S/n-$3"; exit 0 ;;
  "network connect")
    [ -f "$S/n-$3" ] || { echo "Error response from daemon: network $3 not found" >&2; exit 1; }
    echo "$4" >> "$S/att-$3"; exit 0 ;;
  "network disconnect")
    for arg in "$@"; do last2="$last"; last="$arg"; done
    [ -f "$S/att-$last2" ] && grep -vx "$last" "$S/att-$last2" > "$S/att.tmp"; mv "$S/att.tmp" "$S/att-$last2" 2>/dev/null
    exit 0 ;;
  "network ls")
    for f in "$S"/n-*; do [ -f "$f" ] && basename "$f" | sed 's/^n-//'; done; exit 0 ;;
  "image inspect"|"volume ls") exit 0 ;;
esac
case "$1" in
  run)
    name=""; prev=""
    for arg in "$@"; do [ "$prev" = "--name" ] && name="$arg"; prev="$arg"; done
    mkdir -p "$S/c"; touch "$S/c/$name"; printf '%s\n' "$*" > "$S/run-$name"; exit 0 ;;
  rm) for arg in "$@"; do rm -f "$S/c/$arg"; done; exit 0 ;;
  ps)
    net=$(printf '%s\n' "$@" | sed -n 's/^network=//p')
    [ -n "$net" ] && [ -f "$S/att-$net" ] && cat "$S/att-$net"
    exit 0 ;;
  inspect)
    name=""; for arg in "$@"; do name="$arg"; done
    [ -f "$S/c/$name" ] || { echo "Error: No such object: $name" >&2; exit 1; }
    case "$*" in
      *NetworkSettings.Networks*) printf '{}' ;;
      *--format*) printf running ;;
      *) printf '[{"State":{"Status":"running"},"NetworkSettings":{"Ports":{}},"HostConfig":{"PortBindings":{}}}]' ;;
    esac
    exit 0 ;;
esac
exit 0
"#;

/// A Laravel app with no database, so the test is about the swarm only.
const APP: &str = "schema_version: 9\nproject: PROJECT\nproject_type: project\nservices:\n  app:\n    preset: laravel\n";

const WORKSPACE: &str = "schema_version: 9\nproject: ws\nproject_type: project\nswarm:\n  - name: api\n    root: api\n    depends_on: [location]\n    inject_env:\n      - env: LOCATION_URL\n        from: location\n        value: \":url\"\n      - env: LOCATION_HOST\n        from: location\n        value: \":host\"\n  - name: location\n    root: location\n";

struct Workspace {
    base: PathBuf,
}

impl Workspace {
    fn new(label: &str) -> Self {
        let base =
            std::env::temp_dir().join(format!("berth-swarm-ep-{label}-{}", std::process::id()));
        drop(std::fs::remove_dir_all(&base));
        for dir in [
            "bin",
            "state/c",
            "home",
            "user-home",
            "ws/api",
            "ws/location",
        ] {
            std::fs::create_dir_all(base.join(dir)).expect("fixture dir");
        }
        std::fs::write(base.join("ws/.berth.yaml"), WORKSPACE).expect("workspace");
        for project in ["api", "location"] {
            let dir = base.join("ws").join(project);
            std::fs::write(dir.join(".berth.yaml"), APP.replace("PROJECT", project))
                .expect("project");
            std::fs::write(dir.join("artisan"), "#!/usr/bin/env php\n").expect("artisan");
        }
        let docker = base.join("bin/docker");
        std::fs::write(&docker, FAKE_DOCKER).expect("fake docker");
        std::fs::set_permissions(&docker, std::fs::Permissions::from_mode(0o755)).expect("chmod");
        Self { base }
    }

    fn project(&self, name: &str) -> PathBuf {
        self.base.join("ws").join(name)
    }

    fn berth(&self, cwd: &Path, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_berth"))
            .current_dir(cwd)
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
            .stdin(std::process::Stdio::null())
            .output()
            .expect("run berth")
    }

    fn file(&self, name: &str) -> Option<String> {
        std::fs::read_to_string(self.base.join("state").join(name)).ok()
    }

    /// `(container, network)` of the project's app, from its `docker run`.
    fn app(&self, project: &str) -> (String, String) {
        let run = std::fs::read_dir(self.base.join("state"))
            .expect("state")
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .find(|name| {
                name.starts_with(&format!("run-berth_{project}_")) && name.ends_with("_app")
            })
            .expect("app run");
        let container = run.trim_start_matches("run-").to_owned();
        let network = container.trim_end_matches("_app").to_owned();
        (container, network)
    }
}

impl Drop for Workspace {
    fn drop(&mut self) {
        drop(std::fs::remove_dir_all(&self.base));
    }
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

#[test]
fn a_consumer_gets_the_source_container_address_and_joins_its_network() {
    let ws = Workspace::new("join");
    let location = ws.berth(&ws.project("location"), &["up", "--no-wait", "--no-deps"]);
    assert!(location.status.success(), "{}", text(&location.stderr));
    let (source, source_network) = ws.app("location");
    let api = ws.berth(&ws.project("api"), &["up", "--no-wait", "--no-deps"]);
    assert!(api.status.success(), "{}", text(&api.stderr));
    let (consumer, _) = ws.app("api");
    let run = ws.file(&format!("run-{consumer}")).expect("api run");
    assert!(
        run.contains(&format!("LOCATION_URL=http://{source}:80")),
        "an in-network endpoint, not a route the container cannot call: {run}"
    );
    assert!(run.contains(&format!("LOCATION_HOST={source}")), "{run}");
    let endpoint = format!("{source_network}_ep");
    let attached = ws.file(&format!("att-{endpoint}")).unwrap_or_default();
    assert!(
        attached.lines().any(|line| line == consumer),
        "the consumer joins the endpoint network: {attached}"
    );
    assert!(
        attached.lines().any(|line| line == source),
        "so does the source app: {attached}"
    );
    // The source's instance network carries its service aliases (`db`,
    // `redis`); on it the consumer could resolve its own `db` to the
    // source's database by Docker's network-name order.
    let instance = ws
        .file(&format!("att-{source_network}"))
        .unwrap_or_default();
    assert!(
        !instance.lines().any(|line| line == consumer),
        "never the source's instance network: {instance}"
    );
}

#[test]
fn a_consumer_whose_source_is_not_up_fails_precisely_before_starting() {
    let ws = Workspace::new("missing");
    let api = ws.berth(&ws.project("api"), &["up", "--no-wait", "--no-deps"]);
    assert_eq!(api.status.code(), Some(125));
    assert!(
        text(&api.stderr).contains("bring the source up first"),
        "{}",
        text(&api.stderr)
    );
    let started = std::fs::read_dir(ws.base.join("state"))
        .expect("state")
        .filter_map(Result::ok)
        .any(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with("run-berth_api_")
        });
    assert!(!started, "no half-started consumer is left behind");
}

#[test]
fn a_test_run_in_a_consumer_never_needs_its_sources() {
    let ws = Workspace::new("lease");
    let test = ws.berth(
        &ws.project("api"),
        &["artisan", "--no-tty", "--non-interactive", "test"],
    );
    assert!(
        test.status.success(),
        "tests fake their HTTP calls: {}",
        text(&test.stderr)
    );
    let log = ws.file("log").unwrap_or_default();
    assert!(
        !log.lines().any(|line| line.starts_with("network connect")),
        "{log}"
    );
}

#[test]
fn a_consumer_must_depend_on_the_sources_it_injects_from() {
    let ws = Workspace::new("no-depends");
    std::fs::write(
        ws.base.join("ws/.berth.yaml"),
        WORKSPACE.replace("    depends_on: [location]\n", ""),
    )
    .expect("workspace");
    let api = ws.berth(&ws.project("api"), &["up", "--no-wait"]);
    assert_eq!(api.status.code(), Some(125));
    assert!(
        text(&api.stderr).contains("add 'location' to its depends_on"),
        "{}",
        text(&api.stderr)
    );
}

#[test]
fn a_runtime_environment_wires_the_consumer_to_the_same_environment_of_its_source() {
    let ws = Workspace::new("env");
    let location = ws.berth(
        &ws.project("location"),
        &["up", "--no-wait", "--no-deps", "--env", "staging"],
    );
    assert!(location.status.success(), "{}", text(&location.stderr));
    let api = ws.berth(
        &ws.project("api"),
        &["up", "--no-wait", "--no-deps", "--env", "staging"],
    );
    assert!(api.status.success(), "{}", text(&api.stderr));
    let consumer = std::fs::read_dir(ws.base.join("state"))
        .expect("state")
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .find(|name| name.starts_with("run-berth_api_") && name.ends_with("_app-staging"))
        .expect("staging api run");
    let run = ws.file(&consumer).expect("run");
    assert!(
        run.split(' ').any(
            |word| word.starts_with("LOCATION_URL=http://berth_location_")
                && word.ends_with("_app-staging:80")
        ),
        "the staging source: {run}"
    );
    let log = ws.file("log").expect("log");
    assert!(
        log.lines()
            .any(|line| line.starts_with("network connect berth_location_")
                && line.contains("-staging_ep berth_api_")),
        "joins the staging source network: {log}"
    );
}

#[test]
fn purging_a_source_detaches_its_consumers_and_reusing_a_consumer_rejoins() {
    let ws = Workspace::new("purge");
    assert!(
        ws.berth(&ws.project("location"), &["up", "--no-wait", "--no-deps"])
            .status
            .success()
    );
    assert!(
        ws.berth(&ws.project("api"), &["up", "--no-wait", "--no-deps"])
            .status
            .success()
    );
    let (consumer, _) = ws.app("api");
    let (_, instance_network) = ws.app("location");
    let source_network = format!("{instance_network}_ep");
    assert!(
        ws.berth(&ws.project("api"), &["up", "--no-wait", "--no-deps"])
            .status
            .success()
    );
    let joins = ws
        .file("log")
        .expect("log")
        .lines()
        .filter(|line| *line == format!("network connect {source_network} {consumer}"))
        .count();
    assert_eq!(joins, 2, "a reused consumer joins again");
    let purge = ws.berth(&ws.project("location"), &["down", "--purge"]);
    assert!(purge.status.success(), "{}", text(&purge.stderr));
    let log = ws.file("log").expect("log");
    let detach = log
        .lines()
        .position(|line| line == format!("network disconnect --force {source_network} {consumer}"))
        .expect("the consumer is detached");
    let remove = log
        .lines()
        .position(|line| line == format!("network rm {source_network}"))
        .expect("the network is removed");
    assert!(detach < remove, "{log}");
}

#[test]
fn a_recreated_source_rejoins_its_endpoint_network() {
    let ws = Workspace::new("recreate");
    assert!(
        ws.berth(&ws.project("location"), &["up", "--no-wait", "--no-deps"])
            .status
            .success()
    );
    assert!(
        ws.berth(&ws.project("api"), &["up", "--no-wait", "--no-deps"])
            .status
            .success()
    );
    let (source, instance_network) = ws.app("location");
    let endpoint = format!("{instance_network}_ep");
    let recreate = ws.berth(
        &ws.project("location"),
        &["up", "--no-wait", "--no-deps", "--force-recreate"],
    );
    assert!(recreate.status.success(), "{}", text(&recreate.stderr));
    let log = ws.file("log").expect("log");
    let lines: Vec<&str> = log.lines().collect();
    let new_run = lines
        .iter()
        .rposition(|line| line.starts_with("run -d") && line.contains(&format!("--name {source} ")))
        .expect("the source's new run");
    assert!(
        lines[new_run..]
            .iter()
            .any(|line| *line == format!("network connect {endpoint} {source}")),
        "consumers keep reaching a recreated source: {log}"
    );
}

#[test]
fn a_runtime_environment_named_like_a_test_lease_is_still_wired() {
    let ws = Workspace::new("testing-eu");
    let location = ws.berth(
        &ws.project("location"),
        &["up", "--no-wait", "--no-deps", "--env", "testing-eu"],
    );
    assert!(location.status.success(), "{}", text(&location.stderr));
    let api = ws.berth(
        &ws.project("api"),
        &["up", "--no-wait", "--no-deps", "--env", "testing-eu"],
    );
    assert!(api.status.success(), "{}", text(&api.stderr));
    let log = ws.file("log").expect("log");
    assert!(
        log.lines()
            .any(|line| line.starts_with("network connect berth_location_")
                && line.contains("-testing-eu_ep berth_api_")),
        "{log}"
    );
}
