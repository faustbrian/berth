//! `berth down --purge` through the real binary with a fake `docker`.

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
  "network ls")
    for f in "$S"/n-*; do [ -f "$f" ] && basename "$f" | sed 's/^n-//'; done; exit 0 ;;
  "network connect"|"network disconnect") exit 0 ;;
  "image inspect") exit 0 ;;
  "volume rm") [ -f "$S/v-$3" ] || { echo "Error: No such volume: $3" >&2; exit 1; }; rm -f "$S/v-$3"; exit 0 ;;
  "volume ls") for f in "$S"/v-*; do [ -f "$f" ] && basename "$f" | sed 's/^v-//'; done; exit 0 ;;
  "volume inspect")
    vol=""; for arg in "$@"; do vol="$arg"; done
    [ -f "$S/v-$vol" ] || { echo "Error: No such volume: $vol" >&2; exit 1; }; echo "$vol"; exit 0 ;;
  "volume create")
    vol=""; for arg in "$@"; do vol="$arg"; done
    touch "$S/v-$vol"; exit 0 ;;
esac
case "$1" in
  run)
    name=""; prev=""
    : > "$S/labels"
    for arg in "$@"; do
      [ "$prev" = "--name" ] && name="$arg"
      [ "$prev" = "--label" ] && echo "$arg" >> "$S/labels"
      if [ "$prev" = "-v" ]; then
        src="${arg%%:*}"
        case "$src" in */*) ;; *) touch "$S/v-$src" ;; esac
      fi
      prev="$arg"
    done
    mkdir -p "$S/c"; mv "$S/labels" "$S/c/$name"; exit 0 ;;
  rm|stop) [ "$1" = rm ] && for arg in "$@"; do rm -f "$S/c/$arg"; done; exit 0 ;;
  ps)
    for f in "$S"/c/*; do
      [ -f "$f" ] || continue
      keep=1; prev=""
      for arg in "$@"; do
        if [ "$prev" = "--filter" ]; then
          case "$arg" in label=*) grep -qx "${arg#label=}" "$f" || keep=0 ;; esac
        fi
        prev="$arg"
      done
      [ $keep = 1 ] && basename "$f"
    done
    exit 0 ;;
  inspect)
    name=""; for arg in "$@"; do name="$arg"; done
    [ -f "$S/c/$name" ] || exit 1
    case "$*" in
      *NetworkSettings.Networks*) printf '{}' ;;
      *--format*) printf running ;;
      *) printf '[{"State":{"Status":"running"},"NetworkSettings":{"Ports":{}},"HostConfig":{"PortBindings":{}}}]' ;;
    esac
    exit 0 ;;
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
        Self::with_project(
            label,
            "schema_version: 9\nproject: shop\nproject_type: project\nservices:\n  db:\n    preset: postgres\n    isolation: dedicated\n  cache:\n    preset: redis\n    isolation: dedicated\n  app:\n    preset: laravel\n",
        )
    }

    fn with_project(label: &str, project: &str) -> Self {
        let base = std::env::temp_dir().join(format!("berth-purge-{label}-{}", std::process::id()));
        drop(std::fs::remove_dir_all(&base));
        for dir in ["project", "bin", "state/c", "home", "user-home"] {
            std::fs::create_dir_all(base.join(dir)).expect("fixture dir");
        }
        std::fs::write(base.join("project/.berth.yaml"), project).expect("project file");
        std::fs::write(base.join("project/artisan"), "#!/usr/bin/env php\n").expect("artisan");
        let docker = base.join("bin/docker");
        std::fs::write(&docker, FAKE_DOCKER).expect("fake docker");
        std::fs::set_permissions(&docker, std::fs::Permissions::from_mode(0o755)).expect("chmod");
        Self {
            base,
            ports: free_ports(),
        }
    }

    fn berth(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_berth"))
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
            .stdin(std::process::Stdio::null())
            .output()
            .expect("run berth")
    }

    fn volumes(&self) -> Vec<String> {
        let mut volumes: Vec<String> = std::fs::read_dir(self.base.join("state"))
            .expect("state")
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter_map(|name| name.strip_prefix("v-").map(ToOwned::to_owned))
            .collect();
        volumes.sort();
        volumes
    }

    fn calls(&self) -> String {
        std::fs::read_to_string(self.base.join("state/log")).unwrap_or_default()
    }

    fn instance_state_exists(&self) -> bool {
        std::fs::read_dir(self.base.join("home/instances/shop"))
            .is_ok_and(|mut entries| entries.next().is_some())
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

#[test]
fn down_keeps_data_and_purge_deletes_it_in_one_unprompted_call() {
    let fixture = Fixture::new("lifecycle");
    assert!(fixture.berth(&["up", "--no-wait"]).status.success());
    let data = fixture.volumes();
    assert_eq!(data.len(), 2, "postgres and redis data volumes: {data:?}");
    assert!(
        fixture.instance_state_exists(),
        "the managed APP_KEY is kept under ~/.berth"
    );

    let down = fixture.berth(&["down"]);
    assert!(down.status.success(), "{}", text(&down.stderr));
    assert_eq!(fixture.volumes(), data, "down keeps data");
    assert!(fixture.instance_state_exists(), "down keeps credentials");

    let purge = fixture.berth(&["down", "--purge"]);
    assert!(purge.status.success(), "{}", text(&purge.stderr));
    assert!(fixture.volumes().is_empty(), "purge deletes data volumes");
    assert!(
        !fixture.instance_state_exists(),
        "purge deletes the instance's credentials"
    );

    let again = fixture.berth(&["down", "--purge"]);
    assert!(
        again.status.success(),
        "purge is idempotent: {}",
        text(&again.stderr)
    );
}

#[test]
fn purging_one_service_keeps_the_instance_state() {
    let fixture = Fixture::new("subset");
    assert!(fixture.berth(&["up", "--no-wait"]).status.success());
    let purge = fixture.berth(&["down", "--purge", "--service", "cache"]);
    assert!(purge.status.success(), "{}", text(&purge.stderr));
    let volumes = fixture.volumes();
    assert_eq!(volumes.len(), 1, "only cache's data is gone: {volumes:?}");
    assert!(volumes[0].ends_with("_db-data"), "{volumes:?}");
    assert!(
        fixture.instance_state_exists(),
        "other services still need the instance state"
    );
}

#[test]
fn purge_never_touches_a_volume_berth_did_not_create() {
    let fixture = Fixture::with_project(
        "foreign",
        "schema_version: 9\nproject: shop\nproject_type: project\nservices:\n  app:\n    preset: laravel\n    volumes: [\"storage:/app/storage\"]\n",
    );
    std::fs::write(fixture.base.join("state/v-storage"), "").expect("foreign volume");
    assert!(fixture.berth(&["up", "--no-wait"]).status.success());
    let purge = fixture.berth(&["down", "--purge"]);
    assert!(purge.status.success(), "{}", text(&purge.stderr));
    assert!(
        fixture.volumes().contains(&"storage".to_owned()),
        "foreign volume kept"
    );
    assert!(
        !fixture.calls().contains("volume rm storage"),
        "{}",
        fixture.calls()
    );
}

#[test]
fn purge_removes_containers_with_their_anonymous_volumes() {
    let fixture = Fixture::new("anonymous");
    assert!(fixture.berth(&["up", "--no-wait"]).status.success());
    assert!(fixture.berth(&["down", "--purge"]).status.success());
    assert!(
        fixture
            .calls()
            .lines()
            .any(|call| call.starts_with("rm -f -v berth_shop_")),
        "{}",
        fixture.calls()
    );
}

#[test]
fn an_env_purge_keeps_the_default_data_and_a_whole_purge_clears_every_env() {
    let fixture = Fixture::with_project(
        "envs",
        "schema_version: 9\nproject: shop\nproject_type: project\nservices:\n  db:\n    preset: postgres\n    isolation: dedicated\n    volumes: [\"dbdata:/var/lib/postgresql\"]\n",
    );
    assert!(fixture.berth(&["up", "--no-wait"]).status.success());
    assert!(
        fixture
            .berth(&["--env", "staging", "up", "--no-wait"])
            .status
            .success()
    );
    let volumes = fixture.volumes();
    assert_eq!(volumes.len(), 2, "default and staging data: {volumes:?}");

    let staging = fixture.berth(&["--env", "staging", "down", "--purge"]);
    assert!(staging.status.success(), "{}", text(&staging.stderr));
    let left = fixture.volumes();
    assert_eq!(left.len(), 1, "{left:?}");
    assert!(
        !left[0].ends_with("-staging"),
        "the default env's data is kept: {left:?}"
    );

    assert!(
        fixture
            .berth(&["--env", "staging", "up", "--no-wait"])
            .status
            .success()
    );
    let whole = fixture.berth(&["down", "--purge"]);
    assert!(whole.status.success(), "{}", text(&whole.stderr));
    assert!(
        fixture.volumes().is_empty(),
        "every env's data is gone: {:?}",
        fixture.volumes()
    );
}

#[test]
fn purging_an_instance_never_touches_one_whose_id_extends_it() {
    let fixture = Fixture::new("prefix");
    assert!(
        fixture
            .berth(&["--instance", "agent-1-b", "up", "--no-wait"])
            .status
            .success()
    );
    assert!(
        fixture
            .berth(&["--instance", "agent-1-b", "down"])
            .status
            .success()
    );
    let kept = fixture.volumes();
    assert!(
        !kept.is_empty() && kept.iter().all(|volume| volume.contains("agent-1-b")),
        "{kept:?}"
    );
    assert!(
        fixture
            .berth(&["--instance", "agent-1", "up", "--no-wait"])
            .status
            .success()
    );
    let purge = fixture.berth(&["--instance", "agent-1", "down", "--purge"]);
    assert!(purge.status.success(), "{}", text(&purge.stderr));
    assert_eq!(
        fixture.volumes(),
        kept,
        "agent-1-b's data survives agent-1's purge"
    );
}

#[test]
fn dedicated_data_volumes_are_created_with_ownership_labels() {
    let fixture = Fixture::new("labeled");
    assert!(fixture.berth(&["up", "--no-wait"]).status.success());
    let calls = fixture.calls();
    let creates: Vec<&str> = calls
        .lines()
        .filter(|line| line.starts_with("volume create"))
        .collect();
    assert_eq!(creates.len(), 2, "postgres and redis data volumes: {calls}");
    for create in creates {
        assert!(
            create.contains("--label dev.berth.managed=true"),
            "{create}"
        );
        assert!(create.contains("--label dev.berth.instance="), "{create}");
        assert!(create.contains("--label dev.berth.role=volume"), "{create}");
        assert!(create.ends_with("-data"), "{create}");
    }
    // A second `up` reuses them.
    assert!(fixture.berth(&["up", "--no-wait"]).status.success());
    assert_eq!(
        fixture
            .calls()
            .lines()
            .filter(|line| line.starts_with("volume create"))
            .count(),
        2
    );
}
