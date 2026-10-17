//! End-to-end checks of instance selection through the real `berth` binary.

use std::path::{Path, PathBuf};
use std::process::Command;

fn project(label: &str) -> PathBuf {
    let root =
        std::env::temp_dir().join(format!("berth-instance-cli-{label}-{}", std::process::id()));
    drop(std::fs::remove_dir_all(&root));
    std::fs::create_dir_all(&root).expect("create project");
    std::fs::write(
        root.join(".berth.yaml"),
        "schema_version: 9\nproject: shipit-api\nproject_type: project\nservices:\n  db:\n    preset: mysql\n",
    )
    .expect("write project file");
    root
}

fn dry_run_up(root: &Path, instance_env: Option<&str>) -> String {
    let mut command = Command::new(env!("CARGO_BIN_EXE_berth"));
    command
        .current_dir(root)
        .args(["--dry-run", "--non-interactive", "up", "--service", "db"])
        .env_remove("BERTH_INSTANCE");
    if let Some(instance) = instance_env {
        command.env("BERTH_INSTANCE", instance);
    }
    let output = command.output().expect("run berth");
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

#[test]
fn berth_instance_selects_the_container_names() {
    let root = project("env");
    let output = dry_run_up(&root, Some("ci-9"));
    assert!(output.contains("berth_shipit-api_ci-9_db"), "{output}");
    std::fs::remove_dir_all(&root).expect("cleanup");
}

#[test]
fn without_an_instance_the_name_is_derived_from_the_checkout() {
    let root = project("derived");
    let output = dry_run_up(&root, None);
    // The directory `berth-instance-cli-derived-<pid>` slugs to its first 16
    // characters, followed by a 6-digit path hash.
    let prefix = "berth_shipit-api_berth-instance-c-";
    assert!(
        output.contains(prefix) && output.contains("_db"),
        "{output}"
    );
    std::fs::remove_dir_all(&root).expect("cleanup");
}
