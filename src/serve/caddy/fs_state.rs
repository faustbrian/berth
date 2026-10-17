//! Filesystem persistence helpers for Caddy route state/config.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

use super::CaddyState;

/// Returns Berth's Caddy state directory under the user's home directory.
pub(super) fn caddy_dir() -> Result<PathBuf> {
    let home = std::env::var("HOME").context("HOME is not set")?;
    Ok(caddy_dir_with_home(&home))
}

fn caddy_dir_with_home(home: &str) -> PathBuf {
    PathBuf::from(home).join(".config/berth/caddy")
}

/// Returns the Caddy access log path used by Berth.
///
/// # Errors
///
/// Returns an error if HOME is not set.
pub fn caddy_access_log_path() -> Result<PathBuf> {
    Ok(caddy_dir()?.join("access.log"))
}

/// Reads Caddy route state from `sites.toml`, defaulting to empty state.
pub(super) fn read_caddy_state(path: &Path) -> Result<CaddyState> {
    if !path.exists() {
        return Ok(CaddyState::default());
    }

    let content = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    let state: CaddyState =
        toml::from_str(&content).with_context(|| format!("failed to parse {}", path.display()))?;
    Ok(state)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::{
        CaddyState, caddy_access_log_path, caddy_dir, caddy_dir_with_home, read_caddy_state,
    };

    fn write_caddy_state_and_file(
        state_path: &std::path::Path,
        caddyfile_path: &std::path::Path,
        state: &CaddyState,
        caddyfile: &str,
    ) {
        let state_content = toml::to_string_pretty(state).expect("serialize caddy state");
        std::fs::write(state_path, state_content).expect("write state");
        std::fs::write(caddyfile_path, caddyfile).expect("write caddyfile");
    }

    fn temp_home_dir() -> std::path::PathBuf {
        let path =
            std::env::temp_dir().join(format!("berth-caddy-fs-state-{}", std::process::id()));
        drop(std::fs::remove_dir_all(&path));
        std::fs::create_dir_all(&path).expect("create temp home");
        path
    }

    #[test]
    fn caddy_paths_are_under_home_config_directory() {
        let home = temp_home_dir();
        assert_eq!(
            caddy_dir_with_home(&home.to_string_lossy()),
            home.join(".config/berth/caddy")
        );
    }

    #[test]
    fn caddy_access_log_uses_caddy_directory() {
        assert_eq!(
            caddy_access_log_path().expect("access log path"),
            caddy_dir().expect("caddy dir").join("access.log"),
        );
    }

    #[test]
    fn read_caddy_state_defaults_to_empty_state_when_file_missing() {
        let home = temp_home_dir();
        let target = home.join(".config/berth/caddy/sites.toml");
        assert!(!target.exists());

        let state = read_caddy_state(&target).expect("missing state");
        assert!(state.routes.is_empty());
    }

    #[test]
    fn writes_and_reads_caddy_state_and_caddyfile() {
        let home = temp_home_dir();
        let caddy_dir = home.join(".config/berth/caddy");
        std::fs::create_dir_all(&caddy_dir).expect("create caddy dir");
        let state_path = caddy_dir.join("sites.toml");
        let caddyfile_path = caddy_dir.join("Caddyfile");

        let mut routes = BTreeMap::new();
        routes.insert("acme-berth.test".to_owned(), "127.0.0.1:8080".to_owned());
        let state = CaddyState { routes };
        let caddyfile = "test caddyfile content";

        write_caddy_state_and_file(&state_path, &caddyfile_path, &state, caddyfile);

        let written = read_caddy_state(&state_path).expect("read state");
        assert_eq!(written.routes.len(), 1);
        assert_eq!(
            written.routes.get("acme-berth.test").map(String::as_str),
            Some("127.0.0.1:8080")
        );
        let rendered = std::fs::read_to_string(&caddyfile_path).expect("read caddyfile");
        assert_eq!(rendered, caddyfile);
    }
}
