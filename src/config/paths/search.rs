//! config paths search module.
//!
//! Contains config paths search logic used by Berth command workflows.

use anyhow::{Context, Result, anyhow};
use std::path::{Path, PathBuf};

pub(super) fn find_config_file() -> Result<PathBuf> {
    let current_dir = std::env::current_dir().context("failed to get current directory")?;

    find_config_in_path_internal(&current_dir, None)
}

pub(super) fn find_project_root(start: &Path) -> Result<PathBuf> {
    let mut current = start;

    loop {
        if config_path_in_dir(current)?.is_some() {
            return Ok(current.to_path_buf());
        }

        current = current
            .parent()
            .ok_or_else(|| missing_config_error(start))?;
    }
}

pub(super) fn config_path_in_dir(dir: &Path) -> Result<Option<PathBuf>> {
    resolve_config_file(dir, ".berth")
}

#[cfg(test)]
pub(super) fn find_config_in_path(start_path: &Path) -> Result<PathBuf> {
    find_config_in_path_internal(start_path, None)
}

pub(super) fn find_config_in_path_with_env(
    start_path: &Path,
    runtime_env_file: Option<&str>,
) -> Result<PathBuf> {
    find_config_in_path_internal(start_path, runtime_env_file)
}

fn find_config_in_path_internal(
    start_path: &Path,
    runtime_env_file: Option<&str>,
) -> Result<PathBuf> {
    let mut current = start_path;

    loop {
        if let Some(runtime_env_file) = runtime_env_file
            && let Some(runtime_path) = resolve_config_file(current, runtime_env_file)?
        {
            return Ok(runtime_path);
        }

        if let Some(config_path) = config_path_in_dir(current)? {
            return Ok(config_path);
        }

        current = current
            .parent()
            .ok_or_else(|| missing_config_error(start_path))?;
    }
}

fn missing_config_error(start: &Path) -> anyhow::Error {
    anyhow!(
        "no .berth.yaml in {} or any parent directory; run `berth init` in the project to \
         create one, or point berth at it with `--project-root <DIR>` or `--config <FILE>`",
        start.display()
    )
}

/// Resolves `{base_name}.yaml` in `dir`.
///
/// When it is absent but a helm, stackctl or TOML file with the same base
/// exists, resolution fails instead of silently ignoring that file.
fn resolve_config_file(dir: &Path, base_name: &str) -> Result<Option<PathBuf>> {
    let yaml_path = dir.join(format!("{base_name}.yaml"));
    if yaml_path.is_file() {
        return Ok(Some(yaml_path));
    }

    let suffix = base_name.strip_prefix(".berth").unwrap_or(base_name);
    for legacy_base in [".helm", ".stackctl", ".berth"] {
        for extension in ["toml", "yaml"] {
            let legacy = dir.join(format!("{legacy_base}{suffix}.{extension}"));
            if legacy != yaml_path && legacy.is_file() {
                return Err(crate::config::project_file::legacy_config_error(
                    &legacy,
                    &format!("{base_name}.yaml"),
                ));
            }
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::{
        find_config_file, find_config_in_path, find_config_in_path_with_env, find_project_root,
    };

    static TEMP_TREE_COUNTER: AtomicU64 = AtomicU64::new(0);

    const V9_PROJECT: &str = "schema_version: 9\nproject: demo\nproject_type: project\n";

    fn temp_tree() -> std::path::PathBuf {
        let sequence = TEMP_TREE_COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = crate::test_support::temp_root().join(format!(
            "berth-path-search-{}-{}-{sequence}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock")
                .as_nanos()
        ));
        drop(fs::remove_dir_all(&dir));
        fs::create_dir_all(dir.join("nested")).expect("create nested dir");
        dir
    }

    #[test]
    fn find_project_root_climbs_until_config_is_found() {
        let root = temp_tree();
        let nested = root.join("nested");
        fs::write(root.join(".berth.yaml"), V9_PROJECT).expect("seed root config");

        let result = find_project_root(&nested).expect("find root");
        assert_eq!(result, root);
    }

    #[test]
    fn find_config_in_path_supports_direct_match() {
        let root = temp_tree();
        let nested = root.join("nested");
        fs::write(root.join(".berth.yaml"), V9_PROJECT).expect("seed root config");

        let result = find_config_in_path(&nested).expect("find config from nested");
        assert_eq!(result, root.join(".berth.yaml"));
    }

    #[test]
    fn find_config_in_path_with_env_prefers_runtime_file() {
        let root = temp_tree();
        let nested = root.join("nested");
        fs::write(root.join(".berth.yaml"), V9_PROJECT).expect("seed root config");
        fs::write(root.join(".berth.testing.yaml"), V9_PROJECT).expect("seed env config");

        let result = find_config_in_path_with_env(&nested, Some(".berth.testing"))
            .expect("find env config from nested");
        assert_eq!(result, root.join(".berth.testing.yaml"));
    }

    #[test]
    fn find_config_file_uses_current_directory_when_available() {
        let _guard = crate::config::paths::CWD_LOCK.lock().expect("cwd lock");
        let root = temp_tree();
        let cwd = std::env::current_dir().expect("capture cwd");
        let expected = root.join(".berth.yaml");
        fs::write(&expected, V9_PROJECT).expect("seed config in temp");

        std::env::set_current_dir(&root).expect("set cwd to temp");
        let found = find_config_file().expect("find config in cwd");
        std::env::set_current_dir(&cwd).expect("restore cwd");

        assert_eq!(
            found.canonicalize().expect("canonicalize found"),
            expected.canonicalize().expect("canonicalize expected")
        );
    }

    #[test]
    fn config_path_in_dir_returns_yaml_when_present() {
        let root = temp_tree();
        let expected = root.join(".berth.yaml");
        fs::write(&expected, V9_PROJECT).expect("seed yaml config");

        let found = super::config_path_in_dir(&root).expect("resolve config path");
        assert_eq!(found, Some(expected));
    }

    #[test]
    fn legacy_project_file_fails_naming_config_migrate() {
        let root = temp_tree();
        fs::write(root.join(".helm.toml"), "container_prefix = \"api\"\n").expect("seed legacy");

        let error =
            find_project_root(&root.join("nested")).expect_err("legacy file must not be read");
        let message = error.to_string();
        assert!(message.contains(".helm.toml"), "{message}");
        assert!(message.contains("berth config migrate"), "{message}");
    }

    #[test]
    fn legacy_runtime_override_fails_even_when_base_is_migrated() {
        let root = temp_tree();
        fs::write(root.join(".berth.yaml"), V9_PROJECT).expect("seed base");
        fs::write(
            root.join(".stackctl.testing.toml"),
            "container_prefix = \"api\"\n",
        )
        .expect("seed legacy override");

        let error = find_config_in_path_with_env(&root, Some(".berth.testing"))
            .expect_err("an unmigrated override must not be skipped silently");
        assert!(
            error.to_string().contains(".stackctl.testing.toml"),
            "{error}"
        );
    }
}
