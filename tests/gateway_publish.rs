//! Gateway route publication through the real `berth` binary, with a fake
//! `docker` first on PATH that keeps container state and records every call.

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

/// Containers are files `state/c/<name>` holding their labels, one per line.
const FAKE_DOCKER: &str = r#"#!/bin/sh
S="$FAKE_STATE"
echo "$*" >> "$S/log"
label() { grep "^$2=" "$S/c/$1" 2>/dev/null | head -1 | cut -d= -f2-; }
case "$1 $2" in
  "network inspect")
    [ -f "$S/network" ] || exit 1
    case "$*" in *--format*) cat "$BERTH_HOME/installation-id" ;; esac
    exit 0 ;;
  "network create") touch "$S/network"; exit 0 ;;
  "network rm") rm -f "$S/network"; exit 0 ;;
  "network connect")
    net=""; gw=""; for arg in "$@"; do net="$gw"; gw="$arg"; done
    # Only the gateway's networks are modelled; apps join their own.
    case "$gw" in berth_gateway_*) ;; *) exit 0 ;; esac
    mkdir -p "$S/attached"; : > "$S/attached/$net"; echo "$gw" > "$S/gateway-name"
    for arg in "$@"; do case "$arg" in *.berth.localhost) echo "$arg" >> "$S/attached/$net" ;; esac; done
    exit 0 ;;
  "network disconnect")
    net=""; gw=""; for arg in "$@"; do net="$gw"; gw="$arg"; done
    rm -f "$S/attached/$net"; exit 0 ;;
  "image inspect") exit 0 ;;
esac
case "$1" in
  run)
    name=""; prev=""
    : > "$S/run.labels"
    for arg in "$@"; do
      [ "$prev" = "--name" ] && name="$arg"
      [ "$prev" = "--label" ] && echo "$arg" >> "$S/run.labels"
      prev="$arg"
    done
    mkdir -p "$S/c"; mv "$S/run.labels" "$S/c/$name"; exit 0 ;;
  rm)
    case "$*" in *berth_gateway_*) [ -f "$S/gateway-rm-fails" ] && { echo "removal in progress" >&2; exit 1; } ;; esac
    for arg in "$@"; do rm -f "$S/c/$arg"; done; exit 0 ;;
  inspect)
    name=""; for arg in "$@"; do name="$arg"; done
    [ -f "$S/c/$name" ] || exit 1
    case "$*" in
      *NetworkSettings.Networks*)
        # Docker attaches a container started without --network to bridge.
        printf '{"bridge":{"Aliases":null}'; first=0
        for f in "$S"/attached/*; do
          [ -f "$f" ] || continue
          [ $first = 1 ] || printf ','; first=0
          printf '"%s":{"Aliases":[' "$(basename "$f")"
          sep=""; while read -r a; do printf '%s"%s"' "$sep" "$a"; sep=","; done < "$f"
          printf ']}'
        done
        printf '}' ;;
      *State.Status*dev.berth.gateway.spec*) printf 'running %s' "$(label "$name" dev.berth.gateway.spec)" ;;
      *dev.berth.installation*) printf '%s' "$(label "$name" dev.berth.installation)" ;;
      *) printf running ;;
    esac
    exit 0 ;;
  exec)
    case "$*" in
      *"caddy reload"*)
        cat > "$S/reload-stdin"
        [ -n "$FAKE_RELOAD_FAIL" ] && { echo "invalid config" >&2; exit 1; } ;;
    esac
    exit 0 ;;
  ps)
    case "$*" in
      *dev.berth.route.hosts*)
        for f in "$S"/c/*; do
          [ -f "$f" ] || continue
          n=$(basename "$f"); h=$(label "$n" dev.berth.route.hosts)
          [ -n "$h" ] && printf '%s\t%s\t%s\t%s\t%s\n' "$n" "$h" "$(label "$n" dev.berth.route.port)" "$(label "$n" dev.berth.network)" "$(label "$n" dev.berth.route.owned)"
        done ;;
      *network=*)
        net=""; for arg in "$@"; do case "$arg" in network=*) net="${arg#network=}" ;; esac; done
        for f in "$S"/c/*; do [ -f "$f" ] && grep -q "^dev.berth.network=$net$" "$f" && basename "$f"; done
        # The gateway is listed while it is attached, as real Docker does.
        [ -f "$S/attached/$net" ] && cat "$S/gateway-name" ;;
    esac
    exit 0 ;;
esac
exit 0
"#;

struct Fixture {
    base: PathBuf,
    https_port: u16,
}

impl Fixture {
    fn new(label: &str) -> Self {
        let base = std::env::temp_dir().join(format!("berth-gw-{label}-{}", std::process::id()));
        drop(std::fs::remove_dir_all(&base));
        for dir in ["project", "bin", "state/c", "home"] {
            std::fs::create_dir_all(base.join(dir)).expect("fixture dir");
        }
        std::fs::write(
            base.join("project/.berth.yaml"),
            "schema_version: 9\nproject: shop\nproject_type: project\nservices:\n  app:\n    preset: laravel\n",
        )
        .expect("project file");
        let docker = base.join("bin/docker");
        std::fs::write(&docker, FAKE_DOCKER).expect("fake docker");
        std::fs::set_permissions(&docker, std::fs::Permissions::from_mode(0o755)).expect("chmod");
        // The gateway port preflight binds the port, so concurrently running
        // tests need distinct, currently free ports.
        let https_port = free_port();
        Self { base, https_port }
    }

    fn berth(&self, args: &[&str], reload_fails: bool) -> Output {
        let path = format!(
            "{}:{}",
            self.base.join("bin").display(),
            std::env::var("PATH").unwrap_or_default()
        );
        let mut command = Command::new(env!("CARGO_BIN_EXE_berth"));
        command
            .current_dir(self.base.join("project"))
            .args(args)
            .env("PATH", path)
            .env("BERTH_HOME", self.base.join("home"))
            .env("FAKE_STATE", self.base.join("state"))
            .env("BERTH_GATEWAY_HTTPS_PORT", self.https_port.to_string())
            .env("BERTH_GATEWAY_HTTP_PORT", (self.https_port - 1).to_string())
            .env_remove("BERTH_INSTANCE")
            .env_remove("BERTH_ENGINE");
        if reload_fails {
            command.env("FAKE_RELOAD_FAIL", "1");
        }
        command.output().expect("run berth")
    }

    fn berth_with_port(&self, args: &[&str], https_port: u16) -> Output {
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
            .env("BERTH_HOME", self.base.join("home"))
            .env("FAKE_STATE", self.base.join("state"))
            .env("BERTH_GATEWAY_HTTPS_PORT", https_port.to_string())
            // Never the default 8080: a real gateway on the machine may hold
            // it (`free_port` keeps the port below it free too).
            .env("BERTH_GATEWAY_HTTP_PORT", (https_port - 1).to_string())
            .env_remove("BERTH_INSTANCE")
            .env_remove("BERTH_ENGINE")
            .output()
            .expect("run berth")
    }

    fn up_ok(&mut self) {
        let output = self.first_up();
        assert!(output.status.success(), "up failed: {}", stderr(&output));
    }

    /// Runs `up`, moving to another port once if a short-lived host socket
    /// took the chosen one between selection and the gateway preflight. A
    /// failed first start records nothing, so this is a fresh start.
    fn first_up(&mut self) -> Output {
        let output = self.berth(&["--non-interactive", "up", "--no-wait"], false);
        if output.status.success()
            || !stderr(&output).contains("is in use")
            || self.base.join("home/gateway/ports").exists()
        {
            return output;
        }
        self.https_port = free_port();
        self.berth(&["--non-interactive", "up", "--no-wait"], false)
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

    fn document(&self) -> String {
        std::fs::read_to_string(self.base.join("home/gateway/config/caddy.json"))
            .unwrap_or_default()
    }

    fn gateway(&self) -> Option<String> {
        std::fs::read_dir(self.base.join("state/c"))
            .expect("containers")
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .find(|name| name.starts_with("berth_gateway_"))
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        drop(std::fs::remove_dir_all(&self.base));
    }
}

/// Returns a free port whose predecessor is free too, never one handed out
/// before in this process (tests run in parallel and each gateway preflight
/// binds its ports). Ports come from 10000-19999: below the OS ephemeral range
/// and below berth's seeded host-port range (20000-60000), whose allocator in
/// a parallel test would otherwise bind them between the check and use.
fn free_port() -> u16 {
    static NEXT: std::sync::atomic::AtomicU16 = std::sync::atomic::AtomicU16::new(0);
    let base = u16::try_from(std::process::id() % 4000).expect("offset");
    loop {
        let step = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let candidate = 10_001 + ((base + step) % 4995) * 2;
        // A connect probe, not a bind: macOS sets close-on-exec on a new socket
        // only after creating it, so a listener bound here can leak into a
        // `berth` another test thread spawns at that instant and keep the
        // port bound, failing that fixture's gateway with "port in use".
        // A refused connection means nothing listens there.
        let free = |port: u16| std::net::TcpStream::connect(("127.0.0.1", port)).is_err();
        if free(candidate) && free(candidate - 1) {
            return candidate;
        }
    }
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn up_starts_the_gateway_and_applies_the_route_through_a_validated_reload() {
    let mut fixture = Fixture::new("up");
    let output = fixture.first_up();
    assert!(output.status.success(), "{}", stderr(&output));
    let calls = fixture.calls();
    let app_run = calls
        .iter()
        .position(|call| call.starts_with("run") && call.contains("berth_shop_"));
    let gateway_run = calls
        .iter()
        .position(|call| call.starts_with("run") && call.contains("caddy run"));
    let reload = calls.iter().position(|call| call.contains("caddy reload"));
    assert!(app_run < gateway_run && gateway_run < reload, "{calls:#?}");
    // The new config reaches Caddy on stdin, never through the shared
    // config directory: Podman's macOS VM keeps a stale entry for a file the
    // host renamed away and does not show the recreated one.
    let reload_call = &calls[reload.expect("reload")];
    assert!(
        reload_call.starts_with("exec -i ") && reload_call.contains("--config /dev/stdin"),
        "{reload_call}"
    );
    let sent = std::fs::read_to_string(fixture.base.join("state/reload-stdin")).expect("stdin");
    assert!(sent.contains("-shop.berth.localhost"), "{sent}");
    // The gateway publishes on loopback only (FI-40).
    let gateway_call = &calls[gateway_run.expect("gateway run")];
    assert!(
        gateway_call.contains(&format!("-p 127.0.0.1:{}:", fixture.https_port))
            && !gateway_call.contains("-p 0.0.0.0")
            && !gateway_call.contains(&format!("-p {}:", fixture.https_port)),
        "{gateway_call}"
    );
    assert!(
        calls
            .iter()
            .any(|call| call.starts_with("network connect --alias")
                && call.contains("-shop.berth.localhost")),
        "the gateway answers the route inside the instance network: {calls:#?}"
    );
    assert!(
        fixture.document().contains("-shop.berth.localhost"),
        "{}",
        fixture.document()
    );
    // The CA lives in the berth home, mounted at Caddy's data directory, so
    // a recreated gateway keeps it.
    let data = fixture.base.join("home/gateway/data");
    assert!(
        gateway_call.contains(&format!("-v {}:/data", data.display())),
        "{gateway_call}"
    );
    // The route's upstream is this instance's own app container.
    let document: serde_json::Value =
        serde_json::from_str(&fixture.document()).expect("caddy document");
    let app_container = calls[app_run.expect("app run")]
        .split(' ')
        .skip_while(|word| *word != "--name")
        .nth(1)
        .expect("app container name")
        .to_owned();
    let rendered = document.to_string();
    let host = rendered
        .split('"')
        .find(|part| part.ends_with("-shop.berth.localhost"))
        .expect("route host")
        .to_owned();
    let instance = app_container
        .strip_prefix("berth_shop_")
        .and_then(|rest| rest.strip_suffix("_app"))
        .expect("instance in the container name");
    assert_eq!(
        host,
        format!("{instance}-shop.berth.localhost"),
        "{rendered}"
    );
    assert!(
        rendered.contains(&format!("{app_container}:80")),
        "{rendered}"
    );
    assert!(
        !fixture
            .base
            .join("home/gateway/config/caddy.next.json")
            .exists()
    );
}

#[test]
fn a_rejected_reload_fails_up_and_keeps_the_last_accepted_document() {
    let mut fixture = Fixture::new("reload");
    fixture.up_ok();
    let accepted = fixture.document();
    std::fs::remove_file(fixture.base.join("project/.berth.yaml")).expect("config");
    std::fs::write(
        fixture.base.join("project/.berth.yaml"),
        "schema_version: 9\nproject: shop\nproject_type: project\nservices:\n  mailpit:\n    preset: mailpit\n  app:\n    preset: laravel\n",
    )
    .expect("config");

    // Mailpit starts first, so the rejected document adds its route.
    let output = fixture.berth(&["--non-interactive", "up", "--no-wait"], true);
    assert!(!output.status.success(), "a rejected config must fail up");
    assert!(stderr(&output).contains("rejected"), "{}", stderr(&output));
    assert_eq!(
        fixture.document(),
        accepted,
        "the accepted document is never replaced"
    );
    assert!(
        !fixture
            .base
            .join("home/gateway/config/caddy.next.json")
            .exists()
    );
}

#[test]
fn down_never_creates_the_gateway_and_detaches_it_before_removing_the_network() {
    let mut fixture = Fixture::new("down");
    fixture.up_ok();
    let gateway = fixture.gateway().expect("gateway running");

    fixture.clear_log();
    let output = fixture.berth(&["--non-interactive", "down"], false);
    assert!(output.status.success(), "{}", stderr(&output));
    let calls = fixture.calls();
    let detach = calls.iter().position(|call| {
        call.starts_with("network disconnect --force") && call.ends_with(&gateway)
    });
    let remove = calls.iter().position(|call| call.starts_with("network rm"));
    assert!(detach.is_some() && detach < remove, "{calls:#?}");
    assert!(
        !fixture.document().contains("-shop.berth.localhost"),
        "route withdrawn"
    );

    // With no gateway at all, down withdraws routes without starting one.
    fixture.up_ok();
    std::fs::remove_file(fixture.base.join("state/c").join(&gateway)).expect("drop gateway");
    fixture.clear_log();
    let without_gateway = fixture.berth(&["--non-interactive", "down"], false);
    assert!(
        without_gateway.status.success(),
        "{}",
        stderr(&without_gateway)
    );
    assert!(
        !fixture
            .calls()
            .iter()
            .any(|call| call.starts_with("run") && call.contains("caddy run")),
        "{:#?}",
        fixture.calls()
    );
}

#[test]
fn an_authored_domain_never_blocks_a_second_worktree() {
    let mut fixture = Fixture::new("authored");
    std::fs::write(
        fixture.base.join("project/.berth.yaml"),
        "schema_version: 9\nproject: shop\nproject_type: project\nservices:\n  app:\n    preset: laravel\n    domain: shop.berth.localhost\n",
    )
    .expect("config");
    fixture.up_ok();
    assert!(fixture.document().contains("\"shop.berth.localhost\""));

    let second = fixture.berth(
        &[
            "--non-interactive",
            "--instance",
            "agent-2",
            "up",
            "--no-wait",
        ],
        false,
    );
    assert!(second.status.success(), "{}", stderr(&second));
    assert!(stderr(&second).contains("withheld"), "{}", stderr(&second));
    let document = fixture.document();
    assert!(
        document.contains("agent-2-shop.berth.localhost"),
        "{document}"
    );
    assert!(
        !document.contains("\"shop.berth.localhost\""),
        "a shared domain is withheld: {document}"
    );
}

#[test]
fn a_process_without_port_variables_uses_the_recorded_gateway_ports() {
    let mut fixture = Fixture::new("ports");
    fixture.up_ok();
    let listen = format!(":{}", fixture.https_port);
    assert!(fixture.document().contains(&listen));

    let mut command = Command::new(env!("CARGO_BIN_EXE_berth"));
    let output = command
        .current_dir(fixture.base.join("project"))
        .args(["--non-interactive", "down"])
        .env(
            "PATH",
            format!(
                "{}:{}",
                fixture.base.join("bin").display(),
                std::env::var("PATH").unwrap_or_default()
            ),
        )
        .env("BERTH_HOME", fixture.base.join("home"))
        .env("FAKE_STATE", fixture.base.join("state"))
        .env_remove("BERTH_GATEWAY_HTTPS_PORT")
        .env_remove("BERTH_GATEWAY_HTTP_PORT")
        .env_remove("BERTH_INSTANCE")
        .env_remove("BERTH_ENGINE")
        .output()
        .expect("run berth");
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(
        fixture.document().contains(&listen),
        "{}",
        fixture.document()
    );

    let conflicting = fixture.berth_with_port(&["--non-interactive", "up", "--no-wait"], 8443);
    assert!(!conflicting.status.success());
    assert!(
        stderr(&conflicting).contains("8443"),
        "{}",
        stderr(&conflicting)
    );
}

#[test]
fn a_failed_first_gateway_start_does_not_pin_its_ports() {
    // Hold an OS-assigned port first, so nothing can race for it.
    let busy = std::net::TcpListener::bind("127.0.0.1:0").expect("hold port");
    let mut fixture = Fixture::new("busy");
    fixture.https_port = busy.local_addr().expect("addr").port();
    let blocked = fixture.berth(&["--non-interactive", "up", "--no-wait"], false);
    assert!(!blocked.status.success(), "a held port must fail up");
    assert!(stderr(&blocked).contains("in use"), "{}", stderr(&blocked));
    assert!(
        !fixture.base.join("home/gateway/ports").exists(),
        "nothing was served on that port"
    );
    drop(busy);

    let moved = fixture.berth_with_port(&["--non-interactive", "up", "--no-wait"], free_port());
    assert!(moved.status.success(), "{}", stderr(&moved));
    assert!(fixture.base.join("home/gateway/ports").exists());
}

#[test]
fn gc_removes_the_gateway_only_once_no_instance_network_holds_it() {
    let mut fixture = Fixture::new("gc");
    fixture.up_ok();
    let gateway = fixture.gateway().expect("gateway running");

    let kept = fixture.berth(&["gc", "--json"], false);
    assert!(kept.status.success(), "{}", stderr(&kept));
    let kept: serde_json::Value = serde_json::from_slice(&kept.stdout).expect("gc document");
    assert_eq!(kept["gateway"]["removed"], false, "{kept}");
    assert!(
        kept["gateway"]["reason"]
            .as_str()
            .is_some_and(|reason| reason.starts_with("attached to")),
        "{kept}"
    );

    let down = fixture.berth(&["--non-interactive", "down"], false);
    assert!(down.status.success(), "{}", stderr(&down));
    fixture.clear_log();
    let removed = fixture.berth(&["gc", "--json"], false);
    assert!(removed.status.success(), "{}", stderr(&removed));
    let removed: serde_json::Value = serde_json::from_slice(&removed.stdout).expect("gc document");
    assert_eq!(removed["gateway"]["removed"], true, "{removed}");
    assert_eq!(removed["gateway"]["container"], gateway.as_str());
    assert!(
        fixture
            .calls()
            .iter()
            .any(|call| call == &format!("rm -f {gateway}")),
        "{:#?}",
        fixture.calls()
    );
}

#[test]
fn a_failed_gateway_removal_still_reports_everything_gc_did() {
    let mut fixture = Fixture::new("gc-fail");
    fixture.up_ok();
    let down = fixture.berth(&["--non-interactive", "down"], false);
    assert!(down.status.success(), "{}", stderr(&down));
    std::fs::write(fixture.base.join("state/gateway-rm-fails"), b"").expect("fail rm");

    let output = fixture.berth(&["gc", "--json"], false);
    assert_eq!(output.status.code(), Some(125), "{}", stderr(&output));
    let document: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("the gc document is still printed");
    assert_eq!(document["gateway"]["removed"], false, "{document}");
    assert!(
        document["gateway"]["reason"]
            .as_str()
            .is_some_and(|reason| reason.contains("removal in progress")),
        "{document}"
    );
    assert!(document["servers"].is_array(), "{document}");
}

/// The upstream the published gateway document sends `host` to.
fn upstream_of(document: &str, host: &str) -> Option<String> {
    let document: serde_json::Value = serde_json::from_str(document).ok()?;
    let routes = document["apps"]["http"]["servers"]["berth_https"]["routes"].as_array()?;
    routes.iter().find_map(|route| {
        let matches = route["match"][0]["host"]
            .as_array()?
            .iter()
            .any(|value| value == host);
        matches
            .then(|| {
                route["handle"][0]["upstreams"][0]["dial"]
                    .as_str()
                    .map(ToOwned::to_owned)
            })
            .flatten()
    })
}

const ALPHA_ROUTE: &str = "alpha-shop.berth.localhost";
const BETA_AUTHORS_ALPHA: &str = "schema_version: 9\nproject: shop\nproject_type: project\nservices:\n  app:\n    preset: laravel\n    domain: alpha-shop.berth.localhost\n";
const PLAIN: &str = "schema_version: 9\nproject: shop\nproject_type: project\nservices:\n  app:\n    preset: laravel\n";

fn up_instance(fixture: &mut Fixture, instance: &str, project: &str) -> Output {
    std::fs::write(fixture.base.join("project/.berth.yaml"), project).expect("config");
    fixture.berth(
        &[
            "--non-interactive",
            "--instance",
            instance,
            "up",
            "--no-wait",
        ],
        false,
    )
}

#[test]
fn an_authored_domain_never_takes_another_instances_derived_route() {
    // Order (i): alpha serves its route, then beta authors the same host.
    let mut first = Fixture::new("derived-first");
    let alpha = up_instance(&mut first, "alpha", PLAIN);
    assert!(alpha.status.success(), "{}", stderr(&alpha));
    let beta = up_instance(&mut first, "beta", BETA_AUTHORS_ALPHA);
    assert!(beta.status.success(), "{}", stderr(&beta));
    assert!(stderr(&beta).contains("withheld"), "{}", stderr(&beta));
    assert!(stderr(&beta).contains(ALPHA_ROUTE), "{}", stderr(&beta));
    let document = first.document();
    assert_eq!(
        upstream_of(&document, ALPHA_ROUTE).as_deref(),
        Some("berth_shop_alpha_app:80"),
        "{document}"
    );
    assert_eq!(
        upstream_of(&document, "beta-shop.berth.localhost").as_deref(),
        Some("berth_shop_beta_app:80"),
        "{document}"
    );

    // Order (ii): beta authors the host first; alpha still comes up on it.
    let mut second = Fixture::new("authored-first");
    let beta = up_instance(&mut second, "beta", BETA_AUTHORS_ALPHA);
    assert!(beta.status.success(), "{}", stderr(&beta));
    let alpha = up_instance(&mut second, "alpha", PLAIN);
    assert!(alpha.status.success(), "{}", stderr(&alpha));
    assert!(stderr(&alpha).contains("withheld"), "{}", stderr(&alpha));
    assert_eq!(
        upstream_of(&second.document(), ALPHA_ROUTE).as_deref(),
        Some("berth_shop_alpha_app:80"),
        "{}",
        second.document()
    );
}

#[test]
fn another_installations_app_container_is_never_removed() {
    let mut fixture = Fixture::new("foreign-app");
    fixture.up_ok();
    let app = std::fs::read_dir(fixture.base.join("state/c"))
        .expect("containers")
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .find(|name| name.starts_with("berth_shop_") && name.ends_with("_app"))
        .expect("app container");
    let other_home = fixture.base.join("other-home");
    for args in [&["down"][..], &["down", "--purge"][..]] {
        fixture.clear_log();
        let output = Command::new(env!("CARGO_BIN_EXE_berth"))
            .current_dir(fixture.base.join("project"))
            .args(["--non-interactive"].iter().chain(args))
            .env(
                "PATH",
                format!(
                    "{}:{}",
                    fixture.base.join("bin").display(),
                    std::env::var("PATH").unwrap_or_default()
                ),
            )
            .env("BERTH_HOME", &other_home)
            .env("FAKE_STATE", fixture.base.join("state"))
            .env("BERTH_GATEWAY_HTTPS_PORT", fixture.https_port.to_string())
            .env(
                "BERTH_GATEWAY_HTTP_PORT",
                (fixture.https_port - 1).to_string(),
            )
            .env_remove("BERTH_INSTANCE")
            .env_remove("BERTH_ENGINE")
            .output()
            .expect("run berth");
        assert_eq!(
            output.status.code(),
            Some(125),
            "{args:?}: {}",
            stderr(&output)
        );
        assert!(
            stderr(&output).contains("another berth installation"),
            "{}",
            stderr(&output)
        );
        for call in fixture.calls() {
            let verb = call.split(' ').next().unwrap_or_default();
            assert!(
                !(["stop", "rm"].contains(&verb) && call.contains(&app)),
                "{args:?} touched A's app: {call}"
            );
        }
        assert!(fixture.base.join("state/c").join(&app).exists(), "{args:?}");
    }
}

#[test]
fn a_second_instance_spelling_the_same_route_is_refused() {
    // `a-b` of project `c` and `a` of project `b-c` both derive `a-b-c`.
    let mut fixture = Fixture::new("same-route");
    let project = |name: &str| {
        format!(
            "schema_version: 9\nproject: {name}\nproject_type: project\nservices:\n  app:\n    preset: laravel\n"
        )
    };
    let first = up_instance(&mut fixture, "a-b", &project("c"));
    assert!(first.status.success(), "{}", stderr(&first));
    let second = up_instance(&mut fixture, "a", &project("b-c"));
    assert_eq!(second.status.code(), Some(125), "{}", stderr(&second));
    assert!(
        stderr(&second).contains("already served by"),
        "{}",
        stderr(&second)
    );
    assert_eq!(
        upstream_of(&fixture.document(), "a-b-c.berth.localhost").as_deref(),
        Some("berth_c_a-b_app:80"),
        "{}",
        fixture.document()
    );
}

const ROUTED: &str = "schema_version: 9\nproject: shop\nproject_type: project\nservices:\n  app:\n    preset: laravel\n    routes:\n      ee: APP_EE_URL\n      fi: APP_FI_URL\n";

#[test]
fn every_instance_serves_its_own_extra_routes_until_it_goes_down() {
    let mut fixture = Fixture::new("extra-routes");
    let alpha = up_instance(&mut fixture, "alpha", ROUTED);
    assert!(alpha.status.success(), "{}", stderr(&alpha));
    let calls = fixture.calls();
    let alpha_run = calls
        .iter()
        .find(|call| call.starts_with("run") && call.contains("berth_shop_alpha_app"))
        .expect("alpha app run")
        .clone();
    let beta = up_instance(&mut fixture, "beta", ROUTED);
    assert!(beta.status.success(), "{}", stderr(&beta));

    let document = fixture.document();
    let calls = fixture.calls();
    for instance in ["alpha", "beta"] {
        for label in ["ee", "fi"] {
            let host = format!("{instance}-shop-{label}.berth.localhost");
            assert_eq!(
                upstream_of(&document, &host).as_deref(),
                Some(format!("berth_shop_{instance}_app:80").as_str()),
                "{host}: {document}"
            );
            assert!(
                calls
                    .iter()
                    .any(|call| call.starts_with("network connect --alias")
                        && call.contains(&host)
                        && call.contains(&format!("berth_shop_{instance}"))),
                "{host}: {calls:#?}"
            );
        }
    }
    let ee = format!(
        "-e APP_EE_URL=https://alpha-shop-ee.berth.localhost:{}",
        fixture.https_port
    );
    assert!(alpha_run.contains(&ee), "{alpha_run}");
    assert!(
        !alpha_run.contains("--add-host alpha-shop-ee.berth.localhost"),
        "{alpha_run}"
    );

    let down = fixture.berth(&["--non-interactive", "--instance", "alpha", "down"], false);
    assert!(down.status.success(), "{}", stderr(&down));
    let document = fixture.document();
    assert_eq!(
        upstream_of(&document, "alpha-shop-ee.berth.localhost"),
        None
    );
    assert_eq!(
        upstream_of(&document, "beta-shop-ee.berth.localhost").as_deref(),
        Some("berth_shop_beta_app:80"),
        "{document}"
    );
}

#[test]
fn a_runtime_environment_never_claims_the_default_instances_extra_routes() {
    let mut fixture = Fixture::new("extra-routes-env");
    let alpha = up_instance(&mut fixture, "alpha", ROUTED);
    assert!(alpha.status.success(), "{}", stderr(&alpha));
    fixture.clear_log();
    let staging = fixture.berth(
        &[
            "--non-interactive",
            "--instance",
            "alpha",
            "--env",
            "staging",
            "up",
            "--no-wait",
        ],
        false,
    );
    assert!(staging.status.success(), "{}", stderr(&staging));
    assert_eq!(
        upstream_of(&fixture.document(), "alpha-shop-ee.berth.localhost").as_deref(),
        Some("berth_shop_alpha_app:80"),
        "{}",
        fixture.document()
    );
    let calls = fixture.calls();
    let run = calls
        .iter()
        .find(|call| call.starts_with("run") && call.contains("berth_shop_alpha_app-staging"))
        .expect("staging app run");
    assert!(!run.contains("-ee.berth.localhost"), "{run}");
    let value = |key: &str| {
        run.split(" -e ")
            .find_map(|pair| pair.strip_prefix(&format!("{key}=")))
            .and_then(|rest| rest.split(' ').next())
            .map(ToOwned::to_owned)
    };
    assert!(value("APP_URL").is_some(), "{run}");
    assert_eq!(value("APP_EE_URL"), None, "{run}");
}

#[test]
fn a_project_whose_route_is_another_instances_extension_is_refused() {
    // `shop` of instance `alpha` extends its route with `api`; `shop-api`
    // of instance `alpha` derives the same host.
    let mut fixture = Fixture::new("extension-clash");
    let shop = up_instance(
        &mut fixture,
        "alpha",
        "schema_version: 9\nproject: shop\nproject_type: project\nservices:\n  app:\n    preset: laravel\n    routes:\n      api: APP_API_URL\n",
    );
    assert!(shop.status.success(), "{}", stderr(&shop));
    let clash = up_instance(
        &mut fixture,
        "alpha",
        &PLAIN.replace("project: shop", "project: shop-api"),
    );
    assert_eq!(clash.status.code(), Some(125), "{}", stderr(&clash));
    assert!(
        stderr(&clash)
            .contains("alpha-shop-api.berth.localhost is already served by berth_shop_alpha_app"),
        "{}",
        stderr(&clash)
    );
    assert_eq!(
        upstream_of(&fixture.document(), "alpha-shop-api.berth.localhost").as_deref(),
        Some("berth_shop_alpha_app:80"),
        "{}",
        fixture.document()
    );
}
