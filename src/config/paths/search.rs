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

        current = current.parent().ok_or_else(|| {
            anyhow!(
                "no Berth config found (.berth.toml or .berth.yaml) in current directory or any parent directory"
            )
        })?;
    }
}

pub(super) fn config_path_in_dir(dir: &Path) -> Result<Option<PathBuf>> {
    resolve_config_pair(dir, ".berth")
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
        if let Some(runtime_env_file) = runtime_env_file {
            if let Some(runtime_path) = resolve_config_pair(current, runtime_env_file)? {
                return Ok(runtime_path);
            }
        }

        if let Some(config_path) = config_path_in_dir(current)? {
            return Ok(config_path);
        }

        current = current.parent().ok_or_else(|| {
            anyhow!(
                "no Berth config found (.berth.toml or .berth.yaml) in current directory or any parent directory"
            )
        })?;
    }
}

fn resolve_config_pair(dir: &Path, base_name: &str) -> Result<Option<PathBuf>> {
    let toml_path = dir.join(format!("{base_name}.toml"));
    let yaml_path = dir.join(format!("{base_name}.yaml"));

    match (toml_path.exists(), yaml_path.exists()) {
        (true, true) => anyhow::bail!(
            "found both {} and {} in {}; remove one or pass --config explicitly",
            toml_path
                .file_name()
                .and_then(std::ffi::OsStr::to_str)
                .unwrap_or(".berth.toml"),
            yaml_path
                .file_name()
                .and_then(std::ffi::OsStr::to_str)
                .unwrap_or(".berth.yaml"),
            dir.display()
        ),
        (true, false) => Ok(Some(toml_path)),
        (false, true) => Ok(Some(yaml_path)),
        (false, false) => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::{
        find_config_file, find_config_in_path, find_config_in_path_with_env, find_project_root,
    };

    static TEMP_TREE_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn temp_tree() -> std::path::PathBuf {
        let sequence = TEMP_TREE_COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
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
        fs::write(
            root.join(".berth.toml"),
            "schema_version = 1\nproject_type = \"project\"\n",
        )
        .expect("seed root config");

        let result = find_project_root(&nested).expect("find root");
        assert_eq!(result, root);
    }

    #[test]
    fn find_config_in_path_supports_direct_match() {
        let root = temp_tree();
        let nested = root.join("nested");
        fs::write(
            root.join(".berth.toml"),
            "schema_version = 1\nproject_type = \"project\"\n",
        )
        .expect("seed root config");

        let result = find_config_in_path(&nested).expect("find config from nested");
        assert_eq!(result, root.join(".berth.toml"));
    }

    #[test]
    fn find_config_in_path_with_env_prefers_runtime_file() {
        let root = temp_tree();
        let nested = root.join("nested");
        fs::write(
            root.join(".berth.toml"),
            "schema_version = 1\nproject_type = \"project\"\n",
        )
        .expect("seed root config");
        fs::write(
            root.join(".berth.testing.toml"),
            "schema_version = 1\nproject_type = \"project\"\n",
        )
        .expect("seed env config");

        let result = find_config_in_path_with_env(&nested, Some(".berth.testing"))
            .expect("find env config from nested");
        assert_eq!(result, root.join(".berth.testing.toml"));
    }

    #[test]
    fn find_config_file_uses_current_directory_when_available() {
        let _guard = crate::config::paths::CWD_LOCK.lock().expect("cwd lock");
        let root = temp_tree();
        let cwd = std::env::current_dir().expect("capture cwd");
        let expected = root.join(".berth.toml");
        fs::write(
            &expected,
            "schema_version = 1\nproject_type = \"project\"\n",
        )
        .expect("seed config in temp");

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
        fs::write(&expected, "schema_version: 1\nproject_type: project\n")
            .expect("seed yaml config");

        let found = super::config_path_in_dir(&root).expect("resolve config path");
        assert_eq!(found, Some(expected));
    }
}
