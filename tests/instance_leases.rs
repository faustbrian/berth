//! Instance leases, reaping and `berth ls` through the real binary with a
//! fake `docker` (criteria 4 and 10).

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

/// Containers are files in `c/` holding their labels; `ps` filters on
/// labels (`label=key` or `label=key=value`) and renders the catalog
/// format from them.
const FAKE_DOCKER: &str = r#"#!/bin/sh
# The gateway reload sends its config on stdin; take it all.
case "$*" in *"caddy reload"*) cat > /dev/null ;; esac
S="$FAKE_STATE"
echo "$*" >> "$S/log"
case "$1 $2" in
  "network inspect")
    net=""; for arg in "$@"; do net="$arg"; done
    [ -f "$S/n-$net" ] || exit 1
    case "$*" in *--format*) cat "$BERTH_HOME/installation-id" ;; esac
    exit 0 ;;
  "network create")
    net=""; for arg in "$@"; do net="$arg"; done
    touch "$S/n-$net"; exit 0 ;;
  "network rm") rm -f "$S/n-$3"; exit 0 ;;
  "network ls"|"volume ls") exit 0 ;;
  "network connect"|"network disconnect"|"image inspect"|"volume rm") exit 0 ;;
esac
case "$1" in
  run)
    name=""; prev=""
    : > "$S/labels"
    for arg in "$@"; do
      [ "$prev" = "--name" ] && name="$arg"
      [ "$prev" = "--label" ] && echo "$arg" >> "$S/labels"
      prev="$arg"
    done
    mkdir -p "$S/c"; mv "$S/labels" "$S/c/$name"; exit 0 ;;
  rm)
    for arg in "$@"; do
      if [ -f "$S/slow-rm" ] && [ -f "$S/c/$arg" ] && grep -qF "$(cat "$S/slow-rm")" "$S/c/$arg"; then sleep 3; fi
      if [ -f "$S/rm-fails" ] && [ -f "$S/c/$arg" ] && grep -qF "$(cat "$S/rm-fails")" "$S/c/$arg"; then
        echo "Error response from daemon: cannot remove container $arg: device busy" >&2; exit 1
      fi
      rm -f "$S/c/$arg"
    done
    exit 0 ;;
  stop|kill|restart|start|pause|unpause) exit 0 ;;
  ps)
    for f in "$S"/c/*; do
      [ -f "$f" ] || continue
      keep=1; prev=""
      for arg in "$@"; do
        if [ "$prev" = "--filter" ]; then
          case "$arg" in
            label=*=*) grep -qx "${arg#label=}" "$f" || keep=0 ;;
            label=*) grep -q "^${arg#label=}=" "$f" || keep=0 ;;
          esac
        fi
        prev="$arg"
      done
      [ $keep = 1 ] || continue
      case "$*" in
        *'.Label "dev.berth.project"'*)
          get() { grep "^$1=" "$f" | cut -d= -f2-; }
          printf '%s\t%s\t%s\trunning\t%s\n' "$(get dev.berth.project)" "$(get dev.berth.instance)" "$(get dev.berth.root)" "$(basename "$f")" ;;
        *) basename "$f" ;;
      esac
    done
    exit 0 ;;
  inspect)
    [ -f "$S/slow-inspect" ] && sleep 2
    name=""; for arg in "$@"; do name="$arg"; done
    [ -f "$S/c/$name" ] || { echo "Error: No such object: $name" >&2; exit 1; }
    case "$*" in
      *NetworkSettings.Networks*) printf '{}' ;;
      *dev.berth.installation*) grep '^dev.berth.installation=' "$S/c/$name" | cut -d= -f2- | tr -d '\n' ;;
      *--format*) printf running ;;
      *) printf '[{"State":{"Status":"running"},"NetworkSettings":{"Ports":{}},"HostConfig":{"PortBindings":{}}}]' ;;
    esac
    exit 0 ;;
esac
exit 0
"#;

/// A dedicated cache keeps these tests about leases, not shared servers.
const PROJECT: &str = "schema_version: 9\nproject: shop\nproject_type: project\nservices:\n  cache:\n    preset: redis\n    isolation: dedicated\n";

/// Two free loopback ports for this fixture's gateway (see `up_json.rs`).
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

struct Host {
    base: PathBuf,
    ports: (u16, u16),
}

impl Host {
    fn new(label: &str) -> Self {
        let base =
            std::env::temp_dir().join(format!("berth-leases-{label}-{}", std::process::id()));
        drop(std::fs::remove_dir_all(&base));
        for dir in ["bin", "state/c", "home", "user-home"] {
            std::fs::create_dir_all(base.join(dir)).expect("fixture dir");
        }
        let docker = base.join("bin/docker");
        std::fs::write(&docker, FAKE_DOCKER).expect("fake docker");
        std::fs::set_permissions(&docker, std::fs::Permissions::from_mode(0o755)).expect("chmod");
        Self {
            base,
            ports: free_ports(),
        }
    }

    fn checkout(&self, name: &str) -> PathBuf {
        let dir = self.base.join(name);
        std::fs::create_dir_all(&dir).expect("checkout");
        std::fs::write(dir.join(".berth.yaml"), PROJECT).expect("project file");
        dir
    }

    fn berth(&self, cwd: &Path, args: &[&str]) -> Output {
        self.berth_env(cwd, args, &[])
    }

    fn berth_env(&self, cwd: &Path, args: &[&str], env: &[(&str, &str)]) -> Output {
        run_berth(&self.base, self.ports, cwd, args, env)
    }

    /// Lets the next command reap at once, as if the interval had passed.
    fn expire_reap_interval(&self) {
        drop(std::fs::remove_file(
            self.base.join("home/locks/reap.stamp"),
        ));
    }

    fn instances(&self) -> Vec<serde_json::Value> {
        let output = self.berth(&self.base, &["ls", "--json"]);
        assert!(output.status.success(), "{}", text(&output.stderr));
        let document: serde_json::Value =
            serde_json::from_slice(&output.stdout).expect("one JSON document");
        document["instances"].as_array().expect("instances").clone()
    }

    fn containers(&self) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(self.base.join("state/c"))
            .expect("containers")
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }
}

/// Runs the berth binary against the fixture at `base`.
fn run_berth(
    base: &Path,
    ports: (u16, u16),
    cwd: &Path,
    args: &[&str],
    env: &[(&str, &str)],
) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_berth"));
    command
        .current_dir(cwd)
        .args(args)
        .env(
            "PATH",
            format!(
                "{}:{}",
                base.join("bin").display(),
                std::env::var("PATH").unwrap_or_default()
            ),
        )
        .env("HOME", base.join("user-home"))
        .env("BERTH_HOME", base.join("home"))
        .env("BERTH_GATEWAY_HTTPS_PORT", ports.0.to_string())
        .env("BERTH_GATEWAY_HTTP_PORT", ports.1.to_string())
        .env("FAKE_STATE", base.join("state"))
        .env_remove("BERTH_INSTANCE")
        .env_remove("BERTH_ENGINE");
    // Test-specific variables win over the defaults above.
    for &(key, value) in env {
        command.env(key, value);
    }
    command
        .stdin(std::process::Stdio::null())
        .output()
        .expect("run berth")
}

impl Drop for Host {
    fn drop(&mut self) {
        release_ports(self.ports);
        drop(std::fs::remove_dir_all(&self.base));
    }
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn find<'a>(instances: &'a [serde_json::Value], checkout: &str) -> Option<&'a serde_json::Value> {
    instances.iter().find(|row| {
        row["root"]
            .as_str()
            .is_some_and(|root| root.ends_with(&format!("/{checkout}")))
    })
}

#[test]
fn ls_lists_every_instance_across_checkouts_with_its_lease() {
    let host = Host::new("ls");
    let a = host.checkout("a");
    let b = host.checkout("b");
    let up = host.berth(&a, &["up", "--no-wait", "--ttl", "2h"]);
    assert!(up.status.success(), "{}", text(&up.stderr));
    assert!(host.berth(&b, &["up", "--no-wait"]).status.success());
    let instances = host.instances();
    assert_eq!(instances.len(), 2, "{instances:?}");
    let leased = find(&instances, "a").expect("a");
    assert_eq!(leased["project"], "shop");
    assert_eq!(leased["status"], "running");
    assert_eq!(leased["ttl_seconds"], 7200);
    assert!(
        leased["expires_in_seconds"]
            .as_u64()
            .is_some_and(|left| left > 7000)
    );
    assert_eq!(leased["root_exists"], true);
    let forever = find(&instances, "b").expect("b");
    assert!(forever["ttl_seconds"].is_null() && forever["expires_at"].is_null());
    let table = host.berth(&host.base, &["ls"]);
    assert!(
        text(&table.stdout).contains("never") && text(&table.stdout).contains("in 120m"),
        "{}",
        text(&table.stdout)
    );
}

#[test]
fn an_expired_instance_is_reaped_by_the_next_berth_command_elsewhere() {
    let host = Host::new("expired");
    let a = host.checkout("a");
    let b = host.checkout("b");
    assert!(
        host.berth(&a, &["up", "--no-wait", "--ttl", "1s"])
            .status
            .success()
    );
    assert!(host.berth(&b, &["up", "--no-wait"]).status.success());
    std::thread::sleep(std::time::Duration::from_millis(2100));
    host.expire_reap_interval();
    let ps = host.berth(&b, &["ps"]);
    assert!(ps.status.success(), "{}", text(&ps.stderr));
    assert!(
        text(&ps.stderr).contains("because its lease expired"),
        "{}",
        text(&ps.stderr)
    );
    let instances = host.instances();
    assert!(
        find(&instances, "a").is_none(),
        "a was purged: {instances:?}"
    );
    assert!(find(&instances, "b").is_some());
    assert!(
        host.containers().iter().all(|name| !name.contains("_a-")),
        "{:?}",
        host.containers()
    );
}

#[test]
fn a_command_in_an_expired_checkout_renews_it_instead_of_reaping_it() {
    let host = Host::new("renew");
    let a = host.checkout("a");
    assert!(
        host.berth(&a, &["up", "--no-wait", "--ttl", "3s"])
            .status
            .success()
    );
    std::thread::sleep(std::time::Duration::from_millis(3100));
    host.expire_reap_interval();
    let ps = host.berth(&a, &["ps"]);
    assert!(ps.status.success(), "{}", text(&ps.stderr));
    assert!(!text(&ps.stderr).contains("Reaped"), "{}", text(&ps.stderr));
    let instances = host.instances();
    let row = find(&instances, "a").expect("a survives");
    assert_eq!(row["expired"], false, "the lease was renewed: {row}");
    assert_eq!(row["ttl_seconds"], 3, "renewal keeps the TTL: {row}");
    assert!(
        row["expires_in_seconds"]
            .as_u64()
            .is_some_and(|left| left >= 2),
        "{row}"
    );
}

#[test]
fn an_instance_whose_checkout_is_gone_is_reaped() {
    let host = Host::new("gone");
    let a = host.checkout("a");
    let b = host.checkout("b");
    assert!(host.berth(&a, &["up", "--no-wait"]).status.success());
    assert!(host.berth(&b, &["up", "--no-wait"]).status.success());
    std::fs::remove_dir_all(&a).expect("delete the checkout");
    host.expire_reap_interval();
    let instances = host.instances();
    assert!(find(&instances, "a").is_none(), "{instances:?}");
    assert!(find(&instances, "b").is_some());
    assert!(
        !host
            .base
            .join("home/instances/shop")
            .read_dir()
            .expect("state")
            .any(|entry| {
                entry.is_ok_and(|found| found.file_name().to_string_lossy().starts_with("a-"))
            }),
        "its berth state is gone too"
    );
}

#[test]
fn reaping_waits_out_its_interval_and_dry_runs_never_reap() {
    let host = Host::new("interval");
    let a = host.checkout("a");
    let b = host.checkout("b");
    assert!(host.berth(&a, &["up", "--no-wait"]).status.success());
    assert!(host.berth(&b, &["up", "--no-wait"]).status.success());
    std::fs::remove_dir_all(&a).expect("delete the checkout");
    let quick = host.berth(&b, &["ps"]);
    assert!(
        !text(&quick.stderr).contains("Reaped"),
        "within the interval: {}",
        text(&quick.stderr)
    );
    host.expire_reap_interval();
    let dry = host.berth(&b, &["--dry-run", "ps"]);
    assert!(
        !text(&dry.stderr).contains("Reaped"),
        "{}",
        text(&dry.stderr)
    );
    let reaped = host.berth(&b, &["ps"]);
    assert!(
        text(&reaped.stderr).contains("because its checkout is gone"),
        "{}",
        text(&reaped.stderr)
    );
}

#[test]
fn a_dead_test_slot_loses_its_leftover_containers() {
    let host = Host::new("slots");
    let a = host.checkout("a");
    assert!(host.berth(&a, &["up", "--no-wait"]).status.success());
    let cache = host
        .containers()
        .into_iter()
        .find(|name| name.ends_with("_cache"))
        .expect("cache");
    let labels = std::fs::read_to_string(host.base.join("state/c").join(&cache)).expect("labels");
    let key = "0123456789abcdef";
    for slot in ["1", "2"] {
        let name = format!("{cache}-testing-pool-{key}-{slot}");
        std::fs::write(host.base.join("state/c").join(&name), &labels).expect("slot container");
        let locks = host.base.join("home/locks/test-slots").join(key);
        std::fs::create_dir_all(&locks).expect("locks");
        std::fs::write(locks.join(format!("slot-{slot}.lock")), b"").expect("lock file");
    }
    // Hold slot 2's lock as a running test would.
    let holder = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(
            host.base
                .join(format!("home/locks/test-slots/{key}/slot-2.lock")),
        )
        .expect("lock");
    holder.lock().expect("hold slot 2");
    host.expire_reap_interval();
    let ps = host.berth(&a, &["ps"]);
    assert!(ps.status.success(), "{}", text(&ps.stderr));
    let containers = host.containers();
    assert!(
        !containers.iter().any(|name| name.ends_with("-1")),
        "the dead slot's container is gone: {containers:?}"
    );
    assert!(
        containers.iter().any(|name| name.ends_with("-2")),
        "the held slot's container stays: {containers:?}"
    );
    drop(holder);
}

#[test]
fn a_ttl_must_have_a_unit() {
    let host = Host::new("ttl");
    let a = host.checkout("a");
    let up = host.berth(&a, &["up", "--no-wait", "--ttl", "2"]);
    assert_eq!(up.status.code(), Some(125));
    assert!(
        text(&up.stderr).contains("needs a unit"),
        "{}",
        text(&up.stderr)
    );
}

#[test]
fn an_instance_whose_owner_process_died_is_reaped() {
    let host = Host::new("owner");
    let a = host.checkout("a");
    let b = host.checkout("b");
    let mut agent = Command::new("sleep").arg("60").spawn().expect("agent");
    let pid = agent.id().to_string();
    let up = host.berth(&a, &["up", "--no-wait", "--owner-pid", &pid]);
    assert!(up.status.success(), "{}", text(&up.stderr));
    assert!(host.berth(&b, &["up", "--no-wait"]).status.success());
    let row = find(&host.instances(), "a").expect("a").clone();
    assert_eq!(row["owner_pid"].as_u64(), pid.parse().ok());
    assert_eq!(row["owner_alive"], true);
    agent.kill().expect("SIGKILL the agent");
    drop(agent.wait());
    host.expire_reap_interval();
    let ps = host.berth(&b, &["ps"]);
    assert!(
        text(&ps.stderr).contains("because its owner process exited"),
        "{}",
        text(&ps.stderr)
    );
    assert!(find(&host.instances(), "a").is_none());
    let missing = host.berth(&a, &["up", "--no-wait", "--owner-pid", "4000000000"]);
    assert_eq!(
        missing.status.code(),
        Some(125),
        "an owner that does not run is an error"
    );
}

#[test]
fn an_instance_that_cannot_be_reaped_never_blocks_other_commands() {
    let host = Host::new("stuck");
    let a = host.checkout("a");
    let b = host.checkout("b");
    assert!(
        host.berth(&a, &["up", "--no-wait", "--ttl", "1s"])
            .status
            .success()
    );
    let instance = find(&host.instances(), "a").expect("a")["instance"]
        .as_str()
        .expect("id")
        .to_owned();
    std::fs::write(
        host.base.join("state/rm-fails"),
        format!("dev.berth.instance={instance}"),
    )
    .expect("fail rm");
    std::thread::sleep(std::time::Duration::from_millis(2100));
    host.expire_reap_interval();
    let up = host.berth(&b, &["up", "--no-wait"]);
    assert!(
        up.status.success(),
        "another instance's leftovers are not b's failure: {}",
        text(&up.stderr)
    );
    assert!(
        text(&up.stderr).to_lowercase().contains("failed to reap"),
        "{}",
        text(&up.stderr)
    );
    host.expire_reap_interval();
    let gc = host.berth(&host.base, &["gc"]);
    assert_eq!(
        gc.status.code(),
        Some(125),
        "`gc` reports what it could not delete"
    );
}

#[test]
fn an_owner_recorded_in_one_time_zone_is_alive_in_another() {
    let host = Host::new("tz");
    let a = host.checkout("a");
    let mut agent = Command::new("sleep").arg("60").spawn().expect("agent");
    let pid = agent.id().to_string();
    let up = host.berth_env(
        &a,
        &["up", "--no-wait", "--owner-pid", &pid],
        &[("TZ", "UTC")],
    );
    assert!(up.status.success(), "{}", text(&up.stderr));
    host.expire_reap_interval();
    let listing = host.berth_env(
        &host.base,
        &["ls", "--json"],
        &[("TZ", "Pacific/Kiritimati"), ("LC_ALL", "de_DE.UTF-8")],
    );
    assert!(listing.status.success(), "{}", text(&listing.stderr));
    let document: serde_json::Value = serde_json::from_slice(&listing.stdout).expect("ls JSON");
    let row = document["instances"]
        .as_array()
        .and_then(|rows| rows.first())
        .expect("a")
        .clone();
    assert_eq!(row["owner_alive"], true, "{row}");
    assert!(
        !text(&listing.stderr).contains("Reaped"),
        "{}",
        text(&listing.stderr)
    );
    drop(agent.kill());
    drop(agent.wait());
}

fn instance_id(host: &Host, checkout: &str) -> String {
    find(&host.instances(), checkout).expect("instance")["instance"]
        .as_str()
        .expect("id")
        .to_owned()
}

fn one_document(stdout: &[u8]) -> serde_json::Value {
    let mut stream = serde_json::Deserializer::from_slice(stdout).into_iter::<serde_json::Value>();
    let document = stream.next().expect("a document").expect("valid JSON");
    assert!(stream.next().is_none(), "exactly one document");
    document
}

#[test]
fn up_json_prints_its_failure_document_when_the_lease_is_invalid() {
    let host = Host::new("json-lease");
    let a = host.checkout("a");
    for (flag, value, cause) in [
        ("--ttl", "2", "needs a unit"),
        ("--owner-pid", "4000000000", "not running"),
    ] {
        let up = host.berth(&a, &["up", "--json", "--no-wait", flag, value]);
        assert_eq!(up.status.code(), Some(125), "{flag}");
        let document = one_document(&up.stdout);
        assert_eq!(document["status"], "failed", "{flag}: {document}");
        assert!(
            document["error"].to_string().contains(cause),
            "{flag}: {document}"
        );
    }
}

#[test]
fn an_instance_a_command_is_using_is_never_reaped() {
    let host = Host::new("in-use");
    let a = host.checkout("a");
    let b = host.checkout("b");
    assert!(
        host.berth(&a, &["up", "--no-wait", "--ttl", "1s"])
            .status
            .success()
    );
    assert!(host.berth(&b, &["up", "--no-wait"]).status.success());
    let id = instance_id(&host, "a");
    // A long test run in `a` holds the instance in use.
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(
            host.base
                .join(format!("home/locks/instance_shop_{id}.lock")),
        )
        .expect("in-use lock");
    lock.lock_shared().expect("hold a in use");
    std::thread::sleep(std::time::Duration::from_millis(2100));
    host.expire_reap_interval();
    let busy = host.berth(&b, &["ps"]);
    assert!(
        !text(&busy.stderr).contains("Reaped"),
        "{}",
        text(&busy.stderr)
    );
    assert!(find(&host.instances(), "a").is_some(), "a is in use");
    drop(lock);
    host.expire_reap_interval();
    let free = host.berth(&b, &["ps"]);
    assert!(
        text(&free.stderr).contains("because its lease expired"),
        "{}",
        text(&free.stderr)
    );
    assert!(
        !host
            .base
            .join(format!("home/locks/instance_shop_{id}.lock"))
            .exists(),
        "the lock file goes with it"
    );
}

#[test]
fn an_instance_renewed_while_the_reaper_works_survives() {
    let host = Host::new("renewed");
    let a = host.checkout("a");
    let b = host.checkout("b");
    let c = host.checkout("c");
    assert!(
        host.berth(&a, &["up", "--no-wait", "--ttl", "1s"])
            .status
            .success()
    );
    assert!(
        host.berth(&b, &["up", "--no-wait", "--ttl", "60s"])
            .status
            .success()
    );
    assert!(host.berth(&c, &["up", "--no-wait"]).status.success());
    let first = instance_id(&host, "a");
    std::thread::sleep(std::time::Duration::from_millis(2100));
    // b's lease expires in the listing the reaper reads, but b renews it
    // while the reaper is still deleting a (whose removal is slow).
    let lease_path = host.base.join(format!(
        "home/instances/shop/{}/lease.json",
        instance_id(&host, "b")
    ));
    let mut lease: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&lease_path).expect("lease")).expect("json");
    lease["expires_at"] = serde_json::json!(1);
    std::fs::write(&lease_path, lease.to_string()).expect("expire b");
    std::fs::write(
        host.base.join("state/slow-rm"),
        format!("dev.berth.instance={first}"),
    )
    .expect("slow rm");
    host.expire_reap_interval();
    let reaper = std::thread::spawn({
        let base = host.base.clone();
        let ports = host.ports;
        move || run_berth(&base, ports, &c, &["ps"], &[])
    });
    // Renew b only once the reaper is deleting a, after it listed b as
    // expired.
    let started = std::time::Instant::now();
    while !std::fs::read_to_string(host.base.join("state/log"))
        .unwrap_or_default()
        .lines()
        .any(|line| line.starts_with("rm -f -v") && line.contains(&format!("_{first}_")))
    {
        assert!(
            started.elapsed().as_secs() < 30,
            "the reaper never reached a"
        );
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    let b_id = instance_id(&host, "b");
    let renew = host.berth(&b, &["ps"]);
    assert!(renew.status.success(), "{}", text(&renew.stderr));
    let pass = reaper.join().expect("reaper");
    assert!(
        text(&pass.stderr).contains(&format!("Reaped shop/{first}")),
        "{}",
        text(&pass.stderr)
    );
    assert!(
        !text(&pass.stderr).contains(&format!("Reaped shop/{b_id}")),
        "b was renewed before the reaper reached it: {}",
        text(&pass.stderr)
    );
    assert!(find(&host.instances(), "b").is_some());
}

#[test]
fn the_lease_is_renewed_again_when_a_command_ends() {
    let host = Host::new("finish");
    let a = host.checkout("a");
    assert!(
        host.berth(&a, &["up", "--no-wait", "--ttl", "5s"])
            .status
            .success()
    );
    std::fs::write(host.base.join("state/slow-inspect"), b"").expect("slow docker");
    let ps = host.berth(&a, &["ps"]);
    assert!(ps.status.success(), "{}", text(&ps.stderr));
    let ended = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_secs();
    std::fs::remove_file(host.base.join("state/slow-inspect")).expect("fast docker");
    let row = find(&host.instances(), "a").expect("a").clone();
    let expires = row["expires_at"].as_u64().expect("expiry");
    assert!(
        expires + 1 >= ended + 5,
        "the TTL counts from the end of the command: {row}"
    );
}

#[test]
fn listing_services_never_touches_docker_or_leases() {
    let host = Host::new("services");
    let a = host.checkout("a");
    std::fs::write(
        host.base.join("bin/docker"),
        "#!/bin/sh\necho down >&2\nexit 1\n",
    )
    .expect("broken docker");
    let ls = host.berth(&a, &["ls", "--services"]);
    assert!(ls.status.success(), "{}", text(&ls.stderr));
    assert_eq!(text(&ls.stdout).trim(), "cache");
    assert!(
        !text(&ls.stderr).to_lowercase().contains("reap"),
        "{}",
        text(&ls.stderr)
    );
    assert!(
        !host.base.join("home/instances").exists(),
        "no lease is written"
    );
}

#[test]
fn an_unreadable_lease_is_reported_and_never_reaped() {
    let host = Host::new("corrupt");
    let a = host.checkout("a");
    assert!(
        host.berth(&a, &["up", "--no-wait", "--ttl", "1s"])
            .status
            .success()
    );
    let id = instance_id(&host, "a");
    std::fs::write(
        host.base
            .join(format!("home/instances/shop/{id}/lease.json")),
        "{",
    )
    .expect("corrupt");
    std::thread::sleep(std::time::Duration::from_millis(1100));
    host.expire_reap_interval();
    let row = find(&host.instances(), "a")
        .expect("a is still listed")
        .clone();
    assert!(
        row["lease_error"]
            .as_str()
            .is_some_and(|error| error.contains("lease.json")),
        "{row}"
    );
    // Without the guard a deleted checkout would get it reaped.
    std::fs::remove_dir_all(&a).expect("delete the checkout");
    let gc = host.berth(&host.base, &["gc"]);
    assert!(gc.status.success(), "{}", text(&gc.stderr));
    assert!(
        find(&host.instances(), "a").is_some(),
        "an unreadable lease is never reaped"
    );
}

#[test]
fn a_new_up_after_the_owner_died_takes_the_instance_over_instead_of_losing_it() {
    let host = Host::new("takeover");
    let a = host.checkout("a");
    let b = host.checkout("b");
    let mut agent = Command::new("sleep").arg("60").spawn().expect("agent");
    let pid = agent.id().to_string();
    let up = host.berth(&a, &["up", "--no-wait", "--owner-pid", &pid]);
    assert!(up.status.success(), "{}", text(&up.stderr));
    assert!(host.berth(&b, &["up", "--no-wait"]).status.success());
    agent.kill().expect("SIGKILL the agent");
    drop(agent.wait());

    // Someone runs a plain `up` in the checkout before any reap pass: the
    // dead owner must not be carried over, or the next pass deletes it.
    let again = host.berth(&a, &["up", "--no-wait"]);
    assert!(again.status.success(), "{}", text(&again.stderr));
    host.expire_reap_interval();
    let ps = host.berth(&b, &["ps"]);
    assert!(
        !text(&ps.stderr).contains("because its owner process exited"),
        "{}",
        text(&ps.stderr)
    );
    let row = find(&host.instances(), "a").expect("a survives").clone();
    assert_eq!(row["owner_pid"], serde_json::Value::Null, "{row}");
}

#[test]
fn another_installations_instances_are_never_listed_or_reaped() {
    let host = Host::new("two-homes");
    let a = host.checkout("a");
    let b = host.checkout("b");
    // Installation A: an instance whose lease has expired.
    assert!(
        host.berth(&a, &["up", "--no-wait", "--ttl", "1s"])
            .status
            .success()
    );
    std::thread::sleep(std::time::Duration::from_secs(2));
    // Installation B shares the engine (the fake state) but not the home.
    let other_home = host.base.join("other-home");
    let other = other_home.to_str().expect("utf8");
    let up = host.berth_env(&b, &["up", "--no-wait"], &[("BERTH_HOME", other)]);
    assert!(up.status.success(), "{}", text(&up.stderr));
    let listing = host.berth_env(&host.base, &["ls", "--json"], &[("BERTH_HOME", other)]);
    assert!(listing.status.success(), "{}", text(&listing.stderr));
    assert!(
        !text(&listing.stderr).contains("Reaped"),
        "installation B never reaps A's instance: {}",
        text(&listing.stderr)
    );
    let document: serde_json::Value = serde_json::from_slice(&listing.stdout).expect("ls");
    let roots: Vec<&str> = document["instances"]
        .as_array()
        .expect("instances")
        .iter()
        .filter_map(|row| row["root"].as_str())
        .collect();
    assert!(roots.iter().all(|root| !root.ends_with("/a")), "{roots:?}");
    // Installation A's own pass still reaps it.
    host.expire_reap_interval();
    assert!(find(&host.instances(), "a").is_none());
}

#[test]
fn state_of_an_instance_that_never_ran_up_goes_with_its_checkout() {
    let host = Host::new("state-only");
    let c = host.checkout("c");
    std::fs::write(
        c.join(".berth.yaml"),
        "schema_version: 9\nproject: shop\nproject_type: project\nservices:\n  db:\n    preset: postgres\n",
    )
    .expect("shared project");
    // Generated credentials, no lease and no containers.
    let env = host.berth(&c, &["env", "--json"]);
    assert!(env.status.success(), "{}", text(&env.stderr));
    assert!(
        find(&host.instances(), "c").is_some(),
        "listed with its checkout"
    );
    std::fs::remove_dir_all(&c).expect("delete the checkout");
    host.expire_reap_interval();
    let listing = host.berth(&host.base, &["ls", "--json"]);
    assert!(
        text(&listing.stderr).contains("because its checkout is gone"),
        "{}",
        text(&listing.stderr)
    );
    let document: serde_json::Value = serde_json::from_slice(&listing.stdout).expect("ls");
    assert_eq!(
        document["instances"].as_array().map(Vec::len),
        Some(0),
        "{document}"
    );
}

#[test]
fn a_command_that_outlives_its_killed_agent_keeps_the_instance_reapable() {
    let host = Host::new("outlive");
    let a = host.checkout("a");
    let b = host.checkout("b");
    let mut agent = Command::new("sleep").arg("60").spawn().expect("agent");
    let pid = agent.id().to_string();
    assert!(
        host.berth(&a, &["up", "--no-wait", "--owner-pid", &pid])
            .status
            .success()
    );
    assert!(host.berth(&b, &["up", "--no-wait"]).status.success());
    agent.kill().expect("SIGKILL the agent");
    drop(agent.wait());
    // A command the agent started finishes after it died: renewing the
    // lease must keep the dead owner, not clear it.
    let leftover = host.berth(&a, &["ps"]);
    assert!(leftover.status.success(), "{}", text(&leftover.stderr));
    host.expire_reap_interval();
    let ps = host.berth(&b, &["ps"]);
    assert!(
        text(&ps.stderr).contains("because its owner process exited"),
        "{}",
        text(&ps.stderr)
    );
    assert!(find(&host.instances(), "a").is_none());
}

#[test]
fn concurrent_commands_never_drop_a_lease_ttl_set_meanwhile() {
    let host = Host::new("lease-race");
    let a = host.checkout("a");
    assert!(host.berth(&a, &["up", "--no-wait"]).status.success());
    for round in 0..3 {
        std::thread::scope(|scope| {
            let readers: Vec<_> = (0..8)
                .map(|_| scope.spawn(|| host.berth(&a, &["ps"])))
                .collect();
            let ttl = host.berth(&a, &["up", "--no-wait", "--ttl", "1h"]);
            assert!(ttl.status.success(), "{}", text(&ttl.stderr));
            for reader in readers {
                assert!(reader.join().expect("reader").status.success());
            }
        });
        let row = find(&host.instances(), "a").expect("a").clone();
        assert_eq!(row["ttl_seconds"], 3600, "round {round}: {row}");
    }
}

#[test]
fn up_ttl_none_clears_an_existing_ttl_and_keeps_the_owner() {
    let host = Host::new("ttl-none");
    let a = host.checkout("a");
    let b = host.checkout("b");
    let owner = std::process::id().to_string();
    assert!(
        host.berth(
            &a,
            &["up", "--no-wait", "--ttl", "1s", "--owner-pid", &owner]
        )
        .status
        .success()
    );
    let cleared = host.berth(&a, &["up", "--no-wait", "--ttl", "none"]);
    assert!(cleared.status.success(), "{}", text(&cleared.stderr));
    let row = find(&host.instances(), "a").expect("a").clone();
    assert!(row["ttl_seconds"].is_null(), "{row}");
    assert!(row["expires_at"].is_null(), "{row}");
    assert_eq!(row["expired"], false, "{row}");
    assert_eq!(row["owner_pid"].to_string(), owner, "{row}");
    std::thread::sleep(std::time::Duration::from_millis(2100));
    host.expire_reap_interval();
    let ps = host.berth(&b, &["ps"]);
    assert!(ps.status.success(), "{}", text(&ps.stderr));
    assert!(!text(&ps.stderr).contains("Reaped"), "{}", text(&ps.stderr));
    assert!(host.berth(&a, &["ps"]).status.success());
    let row = find(&host.instances(), "a").expect("a still runs").clone();
    assert!(
        row["ttl_seconds"].is_null(),
        "a later command keeps it cleared: {row}"
    );
}

fn calls(host: &Host) -> String {
    std::fs::read_to_string(host.base.join("state/log")).unwrap_or_default()
}

#[test]
fn cp_refuses_a_container_that_is_not_one_of_the_instances_services() {
    let host = Host::new("cp-foreign");
    let a = host.checkout("a");
    let b = host.checkout("b");
    assert!(host.berth(&a, &["up", "--no-wait"]).status.success());
    assert!(host.berth(&b, &["up", "--no-wait"]).status.success());
    let theirs = format!("berth_shop_{}_cache", instance_id(&host, "b"));
    for foreign in [
        theirs.as_str(),
        "postgres",
        "berth_shared_postgres_0123456789ab",
    ] {
        for args in [
            ["cp", &format!("{foreign}:/x"), "./out"],
            ["cp", "./in", &format!("{foreign}:/x")],
        ] {
            let output = host.berth(&a, &args);
            assert_eq!(output.status.code(), Some(125), "{args:?}: {output:?}");
            assert!(
                text(&output.stderr).contains(foreign),
                "{}",
                text(&output.stderr)
            );
        }
    }
    assert!(
        !calls(&host).lines().any(|line| line.starts_with("cp ")),
        "{}",
        calls(&host)
    );
}

#[test]
fn cp_resolves_service_names_and_keeps_host_paths_with_colons() {
    let host = Host::new("cp-own");
    let a = host.checkout("a");
    assert!(host.berth(&a, &["up", "--no-wait"]).status.success());
    let mine = format!("berth_shop_{}_cache", instance_id(&host, "a"));
    for (args, expected) in [
        (
            ["cp", "cache:/data", "./out"],
            format!("cp {mine}:/data ./out"),
        ),
        (
            ["cp", &format!("{mine}:/data"), "./out"],
            format!("cp {mine}:/data ./out"),
        ),
        (["cp", "./a:b", "cache:/x"], format!("cp ./a:b {mine}:/x")),
        (
            ["cp", "/abs/a:b", "cache:/x"],
            format!("cp /abs/a:b {mine}:/x"),
        ),
    ] {
        let output = host.berth(&a, &args);
        assert!(
            output.status.success(),
            "{args:?}: {}",
            text(&output.stderr)
        );
        assert!(
            calls(&host).lines().any(|line| line == expected),
            "{expected}: {}",
            calls(&host)
        );
    }
}

#[test]
fn gc_removes_test_slot_locks_no_run_holds_and_keeps_held_ones() {
    use std::os::unix::fs::MetadataExt;
    let host = Host::new("slot-locks");
    let slots = host.base.join("home/locks/test-slots");
    for (key, slot) in [("k1", 1), ("k1", 2), ("k1", 3), ("k2", 1)] {
        std::fs::create_dir_all(slots.join(key)).expect("slot dir");
        std::fs::write(slots.join(key).join(format!("slot-{slot}.lock")), b"").expect("slot");
    }
    let held_path = slots.join("k1/slot-2.lock");
    let held = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&held_path)
        .expect("open slot");
    held.lock().expect("hold slot 2");
    let gc = host.berth(&host.base, &["gc", "--json"]);
    assert!(gc.status.success(), "{}", text(&gc.stderr));
    let document = one_document(&gc.stdout);
    assert_eq!(document["removed_test_slot_locks"], 3, "{document}");
    for gone in ["k1/slot-1.lock", "k1/slot-3.lock", "k2/slot-1.lock"] {
        assert!(!slots.join(gone).exists(), "{gone}");
    }
    let current = std::fs::metadata(&held_path).expect("held slot kept");
    let mine = held.metadata().expect("held");
    assert_eq!((current.dev(), current.ino()), (mine.dev(), mine.ino()));
    let other = std::fs::File::open(&held_path).expect("reopen");
    assert!(
        matches!(other.try_lock(), Err(std::fs::TryLockError::WouldBlock)),
        "still held"
    );
}

#[test]
fn another_installations_container_is_never_stopped_or_removed() {
    let host = Host::new("foreign-ctr");
    let a = host.checkout("a");
    assert!(host.berth(&a, &["up", "--no-wait"]).status.success());
    let theirs = format!("berth_shop_{}_cache", instance_id(&host, "a"));
    let record = host.base.join("state/c").join(&theirs);
    let label = std::fs::read_to_string(&record).expect("A's container");
    // Installation B shares the engine (the fake state) and the checkout,
    // so every container name matches A's.
    let other_home = host.base.join("other-home");
    let other = other_home.to_str().expect("utf8");
    for args in [
        &["stop"][..],
        &["kill"][..],
        &["pause"][..],
        &["unpause"][..],
        &["wait"][..],
        &["rm", "--force"][..],
        &["restart"][..],
        &["recreate"][..],
        &["down"][..],
        &["down", "--purge"][..],
        &["up", "--no-wait"][..],
    ] {
        std::fs::remove_file(host.base.join("state/log")).expect("clear log");
        let output = host.berth_env(&a, args, &[("BERTH_HOME", other)]);
        assert_eq!(
            output.status.code(),
            Some(125),
            "{args:?}: {}",
            text(&output.stderr)
        );
        assert!(
            text(&output.stderr).contains("another berth installation")
                && text(&output.stderr).contains(&theirs),
            "{args:?}: {}",
            text(&output.stderr)
        );
        for line in calls(&host).lines() {
            let verb = line.split(' ').next().unwrap_or_default();
            assert!(
                !([
                    "stop", "kill", "rm", "restart", "start", "run", "pause", "unpause", "wait",
                    "exec", "volume", "network",
                ]
                .contains(&verb)
                    && line.contains(&theirs)),
                "{args:?} touched A's container: {line}"
            );
        }
        assert_eq!(
            std::fs::read_to_string(&record).expect("A's container survives"),
            label,
            "{args:?}"
        );
    }
    // A's own commands still manage it.
    assert!(host.berth(&a, &["stop"]).status.success());
    let down = host.berth(&a, &["down"]);
    assert!(down.status.success(), "{}", text(&down.stderr));
    assert!(!record.exists(), "A removes its own container");
}

#[test]
fn gc_reaps_a_dead_slots_containers_before_dropping_its_lock_and_keeps_skipped_ones() {
    let host = Host::new("gc-slots");
    let a = host.checkout("a");
    let b = host.checkout("b");
    assert!(host.berth(&a, &["up", "--no-wait"]).status.success());
    assert!(
        host.berth(&b, &["up", "--no-wait", "--ttl", "1s"])
            .status
            .success()
    );
    let key = "0123456789abcdef";
    let locks = host.base.join("home/locks/test-slots").join(key);
    std::fs::create_dir_all(&locks).expect("locks");
    for (checkout, slot) in [("a", "1"), ("b", "2")] {
        let cache = format!("berth_shop_{}_cache", instance_id(&host, checkout));
        let labels =
            std::fs::read_to_string(host.base.join("state/c").join(&cache)).expect("labels");
        let name = format!("{cache}-testing-pool-{key}-{slot}");
        std::fs::write(host.base.join("state/c").join(&name), &labels).expect("slot container");
        std::fs::write(locks.join(format!("slot-{slot}.lock")), b"").expect("lock file");
    }
    // b's lease lapsed, but a command runs in it: the reaper skips it, and
    // its dead slot's container with it.
    std::thread::sleep(std::time::Duration::from_millis(1100));
    let in_use = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(
            host.base
                .join("home/locks")
                .join(format!("instance_shop_{}.lock", instance_id(&host, "b"))),
        )
        .expect("in-use lock");
    in_use.lock_shared().expect("b in use");
    let gc = host.berth(&host.base, &["gc", "--json"]);
    assert!(gc.status.success(), "{}", text(&gc.stderr));
    let document = one_document(&gc.stdout);
    let removed = document["removed_test_containers"].to_string();
    assert!(
        removed.contains("-testing-pool-0123456789abcdef-1"),
        "{document}"
    );
    assert!(
        !locks.join("slot-1.lock").exists(),
        "slot 1 is free and empty: {document}"
    );
    assert!(
        locks.join("slot-2.lock").exists(),
        "slot 2 still has a container a later pass must find: {document}"
    );
    drop(in_use);
}

#[test]
fn a_dead_slots_containers_go_even_when_its_lock_file_is_gone() {
    let host = Host::new("slot-no-lock");
    let a = host.checkout("a");
    assert!(host.berth(&a, &["up", "--no-wait"]).status.success());
    let cache = format!("berth_shop_{}_cache", instance_id(&host, "a"));
    let labels = std::fs::read_to_string(host.base.join("state/c").join(&cache)).expect("labels");
    let name = format!("{cache}-testing-pool-0123456789abcdef-1");
    std::fs::write(host.base.join("state/c").join(&name), &labels).expect("slot container");
    host.expire_reap_interval();
    let ps = host.berth(&a, &["ps"]);
    assert!(ps.status.success(), "{}", text(&ps.stderr));
    assert!(
        !host.containers().contains(&name),
        "{:?}",
        host.containers()
    );
}

#[test]
fn prune_all_never_prunes_another_installations_containers() {
    let host = Host::new("prune-all");
    let a = host.checkout("a");
    assert!(host.berth(&a, &["up", "--no-wait"]).status.success());
    std::fs::remove_file(host.base.join("state/log")).expect("clear log");
    let prune = host.berth(&a, &["prune", "--all", "--force"]);
    assert!(prune.status.success(), "{}", text(&prune.stderr));
    let installation =
        std::fs::read_to_string(host.base.join("home/installation-id")).expect("installation");
    let prunes: Vec<String> = calls(&host)
        .lines()
        .filter(|line| line.starts_with("container prune"))
        .map(ToOwned::to_owned)
        .collect();
    assert_eq!(
        prunes,
        [
            "container prune --force --filter label!=dev.berth.installation".to_owned(),
            format!(
                "container prune --force --filter label=dev.berth.installation={}",
                installation.trim()
            ),
        ],
        "{}",
        calls(&host)
    );
    std::fs::remove_file(host.base.join("state/log")).expect("clear log");
    for filter in ["label!=x", "LABEL!=x"] {
        let negated = host.berth(&a, &["prune", "--all", "--force", "--filter", filter]);
        assert_eq!(
            negated.status.code(),
            Some(125),
            "{}",
            text(&negated.stderr)
        );
        assert!(
            text(&negated.stderr).contains(filter),
            "{}",
            text(&negated.stderr)
        );
    }
    assert!(
        !calls(&host)
            .lines()
            .any(|line| line.starts_with("container prune")),
        "{}",
        calls(&host)
    );
}

#[test]
fn gc_removes_lock_files_of_instances_that_never_existed_and_keeps_held_ones() {
    let host = Host::new("orphan-locks");
    let a = host.checkout("a");
    // `env` marks the instance in use, though nothing else of it exists.
    assert!(host.berth(&a, &["env", "--json"]).status.success());
    let locks = host.base.join("home/locks");
    let orphan: Vec<_> = std::fs::read_dir(&locks)
        .expect("locks")
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with("instance_shop_"))
        .collect();
    assert_eq!(orphan.len(), 1, "{orphan:?}");
    // Another instance a command is using right now keeps its lock.
    let busy = locks.join("instance_shop_busy-0a0b0c.lock");
    std::fs::write(&busy, b"").expect("busy lock");
    let holder = std::fs::File::open(&busy).expect("open");
    holder.lock_shared().expect("in use");
    let gc = host.berth(&host.base, &["gc", "--json"]);
    assert!(gc.status.success(), "{}", text(&gc.stderr));
    assert!(!locks.join(&orphan[0]).exists(), "{orphan:?}");
    assert!(busy.exists(), "a held lock stays");
    drop(holder);
}
