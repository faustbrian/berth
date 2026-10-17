//! `berth config validate` is an effect-free preflight (ledger L004, L005):
//! it checks the project file without a container engine.

// Integration tests are their own crate: panicking on setup failures and
// top-level `#[test]` functions are the idiom here.
#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::tests_outside_test_module
)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

struct Scratch(PathBuf);

impl Scratch {
    fn new(label: &str) -> Self {
        let base = std::env::temp_dir().join(format!(
            "berth-config-validate-{label}-{}",
            std::process::id()
        ));
        drop(std::fs::remove_dir_all(&base));
        std::fs::create_dir_all(base.join("project")).expect("project");
        // First on PATH: a `docker` (and `podman`) that records any call.
        std::fs::create_dir_all(base.join("bin")).expect("bin");
        for engine in ["docker", "podman"] {
            let stub = base.join("bin").join(engine);
            std::fs::write(
                &stub,
                format!(
                    "#!/bin/sh\necho \"$0 $*\" >> '{}'\nexit 1\n",
                    base.join("engine-calls").display()
                ),
            )
            .expect("stub");
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&stub, std::fs::Permissions::from_mode(0o755)).expect("chmod");
        }
        Self(base)
    }

    fn write(&self, content: &str) -> PathBuf {
        let path = self.0.join("project/.berth.yaml");
        std::fs::write(&path, content).expect("project file");
        path
    }

    /// Runs berth from an unrelated directory; the engine stubs record any
    /// call.
    fn berth(&self, args: &[&str]) -> Output {
        let path = format!("{}:/usr/bin:/bin", self.0.join("bin").display());
        Command::new(env!("CARGO_BIN_EXE_berth"))
            .current_dir(&self.0)
            .args(args)
            .env("PATH", path)
            .env("BERTH_HOME", self.0.join("home"))
            .env_remove("BERTH_INSTANCE")
            .output()
            .expect("run berth")
    }

    fn project(&self) -> &Path {
        &self.0
    }

    fn engine_calls(&self) -> Option<String> {
        std::fs::read_to_string(self.0.join("engine-calls")).ok()
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        drop(std::fs::remove_dir_all(&self.0));
    }
}

const VALID: &str = "schema_version: 9\nproject: shop\nproject_type: project\nservices:\n  db:\n    preset: postgres\n  app:\n    preset: laravel\n";

#[test]
fn a_valid_project_file_passes_without_a_container_engine() {
    let scratch = Scratch::new("valid");
    scratch.write(VALID);
    let root = scratch.project().join("project");
    let output = scratch.berth(&[
        "--project-root",
        root.to_str().expect("utf8"),
        "config",
        "validate",
        "--json",
    ]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let document: serde_json::Value = serde_json::from_slice(&output.stdout).expect("document");
    assert_eq!(document["valid"], true);
    assert_eq!(document["project"], "shop");
    assert_eq!(document["services"][0]["name"], "db");
    assert_eq!(document["services"][0]["shared"], true);
    assert_eq!(
        scratch.engine_calls(),
        None,
        "no container engine was called"
    );
}

#[test]
fn a_dependency_cycle_fails_validation_as_it_fails_up() {
    let scratch = Scratch::new("cycle");
    let path = scratch.write(
        "schema_version: 9\nproject: shop\nproject_type: project\nservices:\n  app:\n    preset: laravel\n    depends_on: [worker]\n  worker:\n    preset: horizon\n    depends_on: [app]\n",
    );
    let output = scratch.berth(&[
        "--config",
        path.to_str().expect("utf8"),
        "config",
        "validate",
    ]);
    assert_eq!(output.status.code(), Some(125));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("circular service dependency: app -> worker -> app"),
        "{stderr}"
    );
    assert_eq!(scratch.engine_calls(), None);
}

#[test]
fn an_invalid_project_file_fails_with_125_naming_the_problem() {
    let scratch = Scratch::new("invalid");
    let path = scratch.write(&format!("{VALID}    privileged: true\n"));
    let output = scratch.berth(&[
        "--config",
        path.to_str().expect("utf8"),
        "config",
        "validate",
    ]);
    assert_eq!(output.status.code(), Some(125));
    assert!(output.stdout.is_empty(), "stdout stays empty on failure");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("privileged"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
