//! The managed environment contract through the real `berth` binary: berth
//! never writes `.env`, and what it prints is what app containers receive.

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
  "image inspect") exit 0 ;;
esac
case "$1" in
  run)
    case "$*" in *"--entrypoint printenv"*) echo /home/postgres/pgdata/data; exit 0 ;; esac
    name=""; prev=""
    for arg in "$@"; do [ "$prev" = "--name" ] && name="$arg"; prev="$arg"; done
    # A secret is passed as `-e NAME` with its value in docker's environment;
    # `run-` records what the container receives, `raw-` the argv itself.
    out=""; prev=""
    for arg in "$@"; do
      word="$arg"
      if [ "$prev" = "-e" ] && [ "${arg#*=}" = "$arg" ]; then eval "word=\"$arg=\${$arg}\""; fi
      out="$out $word"; prev="$arg"
    done
    mkdir -p "$S/c"; touch "$S/c/$name"; printf '%s\n' "${out# }" > "$S/run-$name"; printf '%s\n' "$*" > "$S/raw-$name"; exit 0 ;;
  rm) for arg in "$@"; do rm -f "$S/c/$arg"; done; exit 0 ;;
  inspect)
    name=""; for arg in "$@"; do name="$arg"; done
    [ -f "$S/c/$name" ] || exit 1
    case "$*" in
      *NetworkSettings.Networks*) printf '{}' ;;
      *dev.berth.shared.fingerprint*)
        fp=$(grep -o 'dev.berth.shared.fingerprint=[0-9a-f]*' "$S/run-$name" | cut -d= -f2)
        printf 'running\t%s\t%s' "$(cat "$BERTH_HOME/installation-id")" "$fp" ;;
      *dev.berth.installation*) cat "$BERTH_HOME/installation-id" ;;
      *--format*) printf running ;;
      *) printf '[{"State":{"Status":"running"},"NetworkSettings":{"Ports":{}},"HostConfig":{"PortBindings":{}}}]' ;;
    esac
    exit 0 ;;
  exec) grep -q server_version_num && echo 170004; exit 0 ;;
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
    fn new(label: &str, dotenv: Option<&str>) -> Self {
        let base = std::env::temp_dir().join(format!("berth-envc-{label}-{}", std::process::id()));
        drop(std::fs::remove_dir_all(&base));
        for dir in ["project", "bin", "state/c", "home", "user-home"] {
            std::fs::create_dir_all(base.join(dir)).expect("fixture dir");
        }
        std::fs::write(
            base.join("project/.berth.yaml"),
            "schema_version: 9\nproject: shop\nproject_type: project\nservices:\n  db:\n    preset: postgres\n  app:\n    preset: laravel\n    env:\n      APP_URL: http://downgraded.test\n      FOO: bar\n",
        )
        .expect("project file");
        std::fs::write(base.join("project/artisan"), "#!/usr/bin/env php\n").expect("artisan");
        if let Some(contents) = dotenv {
            std::fs::write(base.join("project/.env"), contents).expect(".env");
        }
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
            .env_remove("BERTH_ENGINE")
            .output()
            .expect("run berth")
    }

    fn app_run(&self) -> String {
        std::fs::read_dir(self.base.join("state"))
            .expect("state")
            .filter_map(Result::ok)
            .find(|entry| {
                let name = entry.file_name().to_string_lossy().into_owned();
                name.starts_with("run-") && name.ends_with("_app")
            })
            .map(|entry| std::fs::read_to_string(entry.path()).expect("run"))
            .expect("app run")
    }

    fn dotenv(&self) -> Option<Vec<u8>> {
        std::fs::read(self.base.join("project/.env")).ok()
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

/// `-e KEY=VALUE` pairs of a recorded `docker run`.
fn injected(run: &str) -> Vec<(String, String)> {
    let words: Vec<&str> = run.split(' ').collect();
    words
        .windows(2)
        .filter(|pair| pair[0] == "-e")
        .filter_map(|pair| {
            pair[1]
                .split_once('=')
                .map(|(k, v)| (k.to_owned(), v.to_owned()))
        })
        .collect()
}

#[test]
fn a_fresh_worktree_gets_a_managed_key_and_the_config_cache_bypass_without_a_dotenv() {
    let fixture = Fixture::new("fresh", None);
    let up = fixture.berth(&["up", "--no-wait"]);
    assert!(up.status.success(), "{}", text(&up.stderr));
    assert_eq!(fixture.dotenv(), None, "berth never creates .env");
    let env = injected(&fixture.app_run());
    assert!(
        env.iter()
            .any(|pair| pair.0 == "APP_KEY" && pair.1.starts_with("base64:")),
        "{env:?}"
    );
    assert!(
        env.iter()
            .any(|pair| pair.0 == "APP_CONFIG_CACHE" && pair.1.starts_with("/tmp/")),
        "{env:?}"
    );
    assert!(
        !text(&up.stdout).contains("base64:"),
        "no secrets on stdout of text mode"
    );
    let raw = std::fs::read_dir(fixture.base.join("state"))
        .expect("state")
        .filter_map(Result::ok)
        .find(|entry| {
            entry.file_name().to_string_lossy().starts_with("raw-")
                && entry.file_name().to_string_lossy().ends_with("_app")
        })
        .map(|entry| std::fs::read_to_string(entry.path()).expect("raw"))
        .expect("app argv");
    assert!(
        raw.contains("-e APP_KEY ") && !raw.contains("base64:"),
        "the key never reaches argv: {raw}"
    );
}

#[test]
fn an_existing_dotenv_is_never_written_and_its_key_is_kept() {
    let dotenv = "APP_NAME=mine\nAPP_KEY=base64:mine\nDB_HOST=wrong\n";
    let fixture = Fixture::new("existing", Some(dotenv));
    for args in [
        &["up", "--no-wait"][..],
        &["env"][..],
        &["env", "--json"][..],
        &["down"][..],
    ] {
        let output = fixture.berth(args);
        assert!(
            output.status.success(),
            "{args:?}: {}",
            text(&output.stderr)
        );
        assert_eq!(
            fixture.dotenv().as_deref(),
            Some(dotenv.as_bytes()),
            "{args:?}"
        );
    }
    let up = fixture.berth(&["up", "--no-wait"]);
    assert!(up.status.success());
    let env = injected(&fixture.app_run());
    assert!(
        !env.iter().any(|pair| pair.0 == "APP_KEY"),
        "the project's own key wins: {env:?}"
    );
}

#[test]
fn env_json_reports_exactly_what_the_app_container_receives() {
    let fixture = Fixture::new("json", None);
    assert!(fixture.berth(&["up", "--no-wait"]).status.success());
    let output = fixture.berth(&["env", "--json"]);
    assert!(output.status.success(), "{}", text(&output.stderr));
    let document: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("one JSON document on stdout");
    let environment = document["environment"].as_object().expect("environment");
    for (key, value) in injected(&fixture.app_run()) {
        assert_eq!(
            environment.get(&key).and_then(|v| v.as_str()),
            Some(value.as_str()),
            "{key}"
        );
    }
    assert_eq!(
        environment["DB_HOST"], "db",
        "containers reach the database by name"
    );
    assert_eq!(environment["FOO"], "bar", "explicit app env is included");
    assert!(
        environment["APP_URL"]
            .as_str()
            .is_some_and(|url| url.starts_with("https://")),
        "an http override of the https route is dropped, as in the container: {}",
        environment["APP_URL"]
    );
    assert!(
        document["host"].get("DB_HOST").is_none(),
        "an unpublished database has no host endpoint: {}",
        document["host"]
    );
}

#[test]
fn the_7_0_dotenv_writers_fail_precisely_and_touch_nothing() {
    let dotenv = "APP_NAME=mine\n";
    let fixture = Fixture::new("writers", Some(dotenv));
    for args in [
        &["up", "--no-wait", "--env-output"][..],
        &["env", "--sync"][..],
        &["env", "--purge"][..],
        &["env", "--kind", "app"][..],
        &["env", "generate", "--output", ".env"][..],
    ] {
        let output = fixture.berth(args);
        assert_eq!(output.status.code(), Some(125), "{args:?}");
        let stderr = text(&output.stderr);
        assert!(
            stderr.contains("never") || stderr.contains("already exists"),
            "{args:?}: {stderr}"
        );
        assert_eq!(
            fixture.dotenv().as_deref(),
            Some(dotenv.as_bytes()),
            "{args:?}"
        );
    }
}

#[test]
fn the_app_container_runs_as_the_invoking_user_with_no_spare_capabilities() {
    let fixture = Fixture::new("least-privilege", None);
    let up = fixture.berth(&["up", "--no-wait"]);
    assert!(up.status.success(), "{}", text(&up.stderr));
    let run = fixture.app_run();
    let user = format!(
        "--user {}:{}",
        rustix::process::geteuid().as_raw(),
        rustix::process::getegid().as_raw()
    );
    for expected in [
        user.as_str(),
        "--cap-drop ALL --cap-add NET_BIND_SERVICE",
        "--security-opt no-new-privileges",
        "--tmpfs /data:rw,noexec,nosuid,mode=1777",
        "--tmpfs /config:rw,noexec,nosuid,mode=1777",
        "--env HOME=/home/berth",
    ] {
        assert!(run.contains(expected), "missing {expected}: {run}");
    }
}
