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
    name=""; prev=""
    for arg in "$@"; do [ "$prev" = "--name" ] && name="$arg"; prev="$arg"; done
    mkdir -p "$S/c"; touch "$S/c/$name"; printf '%s\n' "$*" > "$S/run-$name"; exit 0 ;;
  rm) for arg in "$@"; do rm -f "$S/c/$arg"; done; exit 0 ;;
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

struct Fixture {
    base: PathBuf,
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
        Self { base }
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
            .env("FAKE_STATE", self.base.join("state"))
            .env_remove("BERTH_INSTANCE")
            .env_remove("BERTH_GATEWAY_HTTPS_PORT")
            .env_remove("BERTH_GATEWAY_HTTP_PORT")
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
