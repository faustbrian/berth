//! Exit-code and stdin passthrough through the real `berth` binary, with a
//! fake `docker` placed first on PATH.

// Integration tests are their own crate: panicking on setup failures and
// top-level `#[test]` functions are the idiom here.
#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::tests_outside_test_module
)]

use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

const FAKE_DOCKER: &str = r#"#!/bin/sh
# Reports every container as running and runs `exec` according to FAKE_MODE.
case "$1" in
  inspect)
    [ "$FAKE_STATUS" = missing ] && exit 1
    printf '%s' "${FAKE_STATUS:-running}"; exit 0 ;;
  exec)
    [ -n "$FAKE_LOG" ] && echo "$*" >> "$FAKE_LOG"
    # berth's own `sh -lc` compatibility shims succeed unless FAKE_SHIM_FAIL.
    if [ -z "$FAKE_SHIM_FAIL" ]; then
      for arg in "$@"; do [ "$arg" = "-lc" ] && exit 0; done
    fi
    # FAKE_SHIM_FAIL fails only berth's `sh -lc` compatibility shim step.
    if [ -n "$FAKE_SHIM_FAIL" ]; then
      for arg in "$@"; do [ "$arg" = "-lc" ] && exit 7; done
    fi
    case "$FAKE_MODE" in
      exit) exit "$FAKE_CODE" ;;
      kill) kill -9 $$ ;;
      cat) cat ;;
    esac
    exit 0 ;;
esac
exit 0
"#;

struct Fixture {
    root: PathBuf,
    bin: PathBuf,
}

impl Fixture {
    fn new(label: &str) -> Self {
        let base = std::env::temp_dir().join(format!("berth-exit-{label}-{}", std::process::id()));
        drop(std::fs::remove_dir_all(&base));
        let root = base.join("project");
        let bin = base.join("bin");
        std::fs::create_dir_all(&root).expect("project dir");
        std::fs::create_dir_all(&bin).expect("bin dir");
        std::fs::write(
            root.join(".berth.yaml"),
            "schema_version: 9\nproject: api\nproject_type: project\nservices:\n  app:\n    preset: laravel\n",
        )
        .expect("project file");
        let docker = bin.join("docker");
        std::fs::write(&docker, FAKE_DOCKER).expect("fake docker");
        std::fs::set_permissions(&docker, std::fs::Permissions::from_mode(0o755)).expect("chmod");
        Self { root, bin }
    }

    fn berth(&self, args: &[&str], mode: &str, code: &str, stdin: Option<&[u8]>) -> Output {
        let path = format!(
            "{}:{}",
            self.bin.display(),
            std::env::var("PATH").unwrap_or_default()
        );
        let mut child = Command::new(env!("CARGO_BIN_EXE_berth"))
            .current_dir(&self.root)
            .args(args)
            .env("PATH", path)
            .env("FAKE_MODE", mode)
            .env("FAKE_CODE", code)
            // Never the developer's own berth home.
            .env("BERTH_HOME", self.root.parent().expect("base").join("home"))
            .env_remove("BERTH_INSTANCE")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn berth");
        let mut input = child.stdin.take().expect("stdin");
        if let Some(bytes) = stdin {
            input.write_all(bytes).expect("write stdin");
        }
        drop(input);
        child.wait_with_output().expect("berth output")
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        if let Some(base) = self.root.parent() {
            drop(std::fs::remove_dir_all(base));
        }
    }
}

fn exec(fixture: &Fixture, mode: &str, code: &str) -> Output {
    fixture.berth(
        &["--non-interactive", "exec", "--no-tty", "--", "true"],
        mode,
        code,
        None,
    )
}

#[test]
fn the_command_exit_code_is_berths_exit_code() {
    let fixture = Fixture::new("codes");
    for code in [0, 1, 7, 255] {
        let output = exec(&fixture, "exit", &code.to_string());
        assert_eq!(output.status.code(), Some(code), "{output:?}");
    }
}

#[test]
fn a_command_killed_by_a_signal_exits_128_plus_the_signal() {
    let fixture = Fixture::new("signal");
    let output = exec(&fixture, "kill", "0");
    assert_eq!(output.status.code(), Some(137), "{output:?}");
}

#[test]
fn piped_stdin_reaches_the_command() {
    let fixture = Fixture::new("stdin");
    let payload = b"select 1;\nselect 2;\n";
    let output = fixture.berth(
        &["--non-interactive", "exec", "--no-tty", "--", "cat"],
        "cat",
        "0",
        Some(payload),
    );
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert_eq!(output.stdout, payload);
}

#[test]
fn berths_own_failures_exit_125_on_stderr_only() {
    let base = std::env::temp_dir().join(format!("berth-exit-noproject-{}", std::process::id()));
    std::fs::create_dir_all(&base).expect("dir");
    let output = Command::new(env!("CARGO_BIN_EXE_berth"))
        .current_dir(&base)
        .args(["ps"])
        .env_remove("BERTH_INSTANCE")
        .output()
        .expect("run berth");
    assert_eq!(output.status.code(), Some(125), "{output:?}");
    assert!(
        output.stdout.is_empty(),
        "errors must not pollute stdout: {output:?}"
    );
    assert!(String::from_utf8_lossy(&output.stderr).contains("no berth project file"));
    drop(std::fs::remove_dir_all(&base));
}

#[test]
fn progress_events_go_to_stderr_and_stdout_stays_clean() {
    let fixture = Fixture::new("streams");
    let output = fixture.berth(&["--dry-run", "--non-interactive", "up"], "exit", "0", None);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert!(
        output.stdout.is_empty(),
        "stdout: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("[dry-run]"),
        "{output:?}"
    );
}

#[test]
fn exec_against_a_stopped_or_missing_instance_is_a_berth_failure() {
    let fixture = Fixture::new("stopped");
    let path = format!(
        "{}:{}",
        fixture.bin.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    for status in ["exited", "missing"] {
        let output = Command::new(env!("CARGO_BIN_EXE_berth"))
            .current_dir(&fixture.root)
            .args(["--non-interactive", "exec", "--no-tty", "--", "true"])
            .env("PATH", &path)
            .env("FAKE_STATUS", status)
            .env("FAKE_MODE", "exit")
            .env("FAKE_CODE", "1")
            .env_remove("BERTH_INSTANCE")
            .output()
            .expect("run berth");
        assert_eq!(
            output.status.code(),
            Some(125),
            "status {status:?}: {output:?}"
        );
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("berth up"),
            "{output:?}"
        );
    }
}

#[test]
fn usage_errors_exit_125_and_help_exits_0() {
    let output = Command::new(env!("CARGO_BIN_EXE_berth"))
        .args(["exec", "--no-ttyy"])
        .output()
        .expect("run berth");
    assert_eq!(output.status.code(), Some(125), "{output:?}");
    let help = Command::new(env!("CARGO_BIN_EXE_berth"))
        .arg("--help")
        .output()
        .expect("help");
    assert_eq!(help.status.code(), Some(0));
}

#[test]
fn a_failing_setup_shim_is_a_berth_failure_not_the_commands_exit() {
    let fixture = Fixture::new("shim");
    let path = format!(
        "{}:{}",
        fixture.bin.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let output = Command::new(env!("CARGO_BIN_EXE_berth"))
        .current_dir(&fixture.root)
        .args(["--non-interactive", "artisan", "--no-tty", "about"])
        .env("PATH", &path)
        .env("FAKE_MODE", "exit")
        .env("FAKE_CODE", "0")
        .env("FAKE_SHIM_FAIL", "1")
        .env_remove("BERTH_INSTANCE")
        .output()
        .expect("run berth");
    assert_eq!(output.status.code(), Some(125), "{output:?}");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("mysql compatibility wrapper"),
        "the failure must come from the shim: {output:?}"
    );
}

#[test]
fn package_manager_and_php_tool_commands_keep_their_exit_codes() {
    let fixture = Fixture::new("tools");
    for args in [
        &["--non-interactive", "composer", "--no-tty", "validate"][..],
        &["--non-interactive", "pest", "--no-tty"][..],
        &["--non-interactive", "phpstan", "--no-tty", "analyse"][..],
    ] {
        for code in ["7", "1"] {
            let output = fixture.berth(args, "exit", code, None);
            assert_eq!(
                output.status.code(),
                code.parse().ok(),
                "{args:?}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        let killed = fixture.berth(args, "kill", "0", None);
        assert_eq!(killed.status.code(), Some(137), "{args:?}");
    }
}

#[test]
fn without_a_terminal_commands_run_without_a_tty_by_default() {
    let fixture = Fixture::new("tty-default");
    let log = fixture.root.join("exec.log");
    let path = format!(
        "{}:{}",
        fixture.bin.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    // No --tty, --no-tty or --non-interactive: stdin is a pipe.
    let output = Command::new(env!("CARGO_BIN_EXE_berth"))
        .current_dir(&fixture.root)
        .args(["exec", "--", "true"])
        .env("PATH", path)
        .env("FAKE_MODE", "exit")
        .env("FAKE_CODE", "0")
        .env("FAKE_LOG", &log)
        .env(
            "BERTH_HOME",
            fixture.root.parent().expect("base").join("home"),
        )
        .env_remove("BERTH_INSTANCE")
        .stdin(Stdio::piped())
        .output()
        .expect("run berth");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let calls = std::fs::read_to_string(&log).expect("exec log");
    assert!(
        calls.lines().any(|line| line.starts_with("exec -i ")),
        "{calls}"
    );
    assert!(!calls.contains("-it"), "{calls}");
}
