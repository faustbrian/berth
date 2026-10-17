//! Transactional Caddy config apply with rollback and interrupted-run recovery.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

use crate::serve::CaddyState;

use super::process;

pub(super) fn apply_caddy_state(
    caddy_dir: &Path,
    state: &CaddyState,
    caddyfile: &str,
) -> Result<()> {
    let controller = SystemCaddyProcessController;
    apply_caddy_state_with_controller(caddy_dir, state, caddyfile, &controller)
}

trait CaddyProcessController {
    fn ensure_installed(&self) -> Result<()>;
    fn validate_config(&self, caddyfile_path: &Path) -> Result<()>;
    fn reload_or_start(&self, caddyfile_path: &Path) -> Result<()>;
}

struct SystemCaddyProcessController;

impl CaddyProcessController for SystemCaddyProcessController {
    fn ensure_installed(&self) -> Result<()> {
        process::ensure_caddy_installed()
    }

    fn validate_config(&self, caddyfile_path: &Path) -> Result<()> {
        process::validate_caddy_config(caddyfile_path)
    }

    fn reload_or_start(&self, caddyfile_path: &Path) -> Result<()> {
        process::reload_or_start_caddy(caddyfile_path)
    }
}

fn apply_caddy_state_with_controller(
    caddy_dir: &Path,
    state: &CaddyState,
    caddyfile: &str,
    controller: &impl CaddyProcessController,
) -> Result<()> {
    let mut transaction = CaddyApplyTransaction::prepare(caddy_dir, state, caddyfile)?;

    controller.ensure_installed()?;
    controller.validate_config(transaction.staged_caddyfile_path())?;
    transaction.swap_live_files()?;

    if let Err(apply_error) = controller.reload_or_start(transaction.live_caddyfile_path()) {
        let rollback_error = transaction.rollback().err();
        let restart_error = if transaction.has_live_caddyfile() {
            controller
                .reload_or_start(transaction.live_caddyfile_path())
                .err()
        } else {
            None
        };
        let rollback_suffix = rollback_error
            .as_ref()
            .map(|error| format!("\nrollback error: {error}"))
            .unwrap_or_default();
        let restart_suffix = restart_error
            .as_ref()
            .map(|error| format!("\nrestart error: {error}"))
            .unwrap_or_default();
        anyhow::bail!(
            "failed to apply caddy config: {apply_error}{rollback_suffix}{restart_suffix}"
        );
    }

    transaction.finish()
}

struct CaddyApplyTransaction {
    live_state_path: PathBuf,
    live_caddyfile_path: PathBuf,
    staged_state_path: PathBuf,
    staged_caddyfile_path: PathBuf,
    backup_state_path: PathBuf,
    backup_caddyfile_path: PathBuf,
    state_had_live_file: bool,
    caddyfile_had_live_file: bool,
}

impl CaddyApplyTransaction {
    fn prepare(caddy_dir: &Path, state: &CaddyState, caddyfile: &str) -> Result<Self> {
        std::fs::create_dir_all(caddy_dir)
            .with_context(|| format!("failed to create {}", caddy_dir.display()))?;

        let live_state_path = caddy_dir.join("sites.toml");
        let live_caddyfile_path = caddy_dir.join("Caddyfile");
        let staged_state_path = caddy_dir.join("sites.toml.staged");
        let staged_caddyfile_path = caddy_dir.join("Caddyfile.staged");
        let backup_state_path = caddy_dir.join("sites.toml.bak");
        let backup_caddyfile_path = caddy_dir.join("Caddyfile.bak");

        recover_interrupted_apply(&live_state_path, &backup_state_path)?;
        recover_interrupted_apply(&live_caddyfile_path, &backup_caddyfile_path)?;
        remove_file_if_exists(&staged_state_path)?;
        remove_file_if_exists(&staged_caddyfile_path)?;
        remove_file_if_exists(&backup_state_path)?;
        remove_file_if_exists(&backup_caddyfile_path)?;

        let state_content =
            toml::to_string_pretty(state).context("failed to serialize caddy state")?;
        std::fs::write(&staged_state_path, state_content)
            .with_context(|| format!("failed to write {}", staged_state_path.display()))?;
        std::fs::write(&staged_caddyfile_path, caddyfile)
            .with_context(|| format!("failed to write {}", staged_caddyfile_path.display()))?;

        Ok(Self {
            state_had_live_file: live_state_path.exists(),
            caddyfile_had_live_file: live_caddyfile_path.exists(),
            live_state_path,
            live_caddyfile_path,
            staged_state_path,
            staged_caddyfile_path,
            backup_state_path,
            backup_caddyfile_path,
        })
    }

    fn staged_caddyfile_path(&self) -> &Path {
        &self.staged_caddyfile_path
    }

    fn live_caddyfile_path(&self) -> &Path {
        &self.live_caddyfile_path
    }

    fn has_live_caddyfile(&self) -> bool {
        self.live_caddyfile_path.exists()
    }

    fn swap_live_files(&mut self) -> Result<()> {
        replace_live_file(
            &self.live_state_path,
            &self.staged_state_path,
            &self.backup_state_path,
            self.state_had_live_file,
        )?;

        if let Err(error) = replace_live_file(
            &self.live_caddyfile_path,
            &self.staged_caddyfile_path,
            &self.backup_caddyfile_path,
            self.caddyfile_had_live_file,
        ) {
            self.rollback().with_context(|| {
                format!(
                    "failed to restore caddy state after swap error for {}",
                    self.live_caddyfile_path.display()
                )
            })?;
            return Err(error);
        }

        Ok(())
    }

    fn rollback(&mut self) -> Result<()> {
        restore_live_file(
            &self.live_caddyfile_path,
            &self.backup_caddyfile_path,
            self.caddyfile_had_live_file,
        )?;
        restore_live_file(
            &self.live_state_path,
            &self.backup_state_path,
            self.state_had_live_file,
        )?;
        remove_file_if_exists(&self.staged_state_path)?;
        remove_file_if_exists(&self.staged_caddyfile_path)?;
        Ok(())
    }

    fn finish(self) -> Result<()> {
        remove_file_if_exists(&self.backup_state_path)?;
        remove_file_if_exists(&self.backup_caddyfile_path)?;
        remove_file_if_exists(&self.staged_state_path)?;
        remove_file_if_exists(&self.staged_caddyfile_path)?;
        Ok(())
    }
}

fn recover_interrupted_apply(live_path: &Path, backup_path: &Path) -> Result<()> {
    if live_path.exists() || !backup_path.exists() {
        return Ok(());
    }

    std::fs::rename(backup_path, live_path).with_context(|| {
        format!(
            "failed to restore {} from {}",
            live_path.display(),
            backup_path.display()
        )
    })
}

fn replace_live_file(
    live_path: &Path,
    staged_path: &Path,
    backup_path: &Path,
    had_live_file: bool,
) -> Result<()> {
    if had_live_file {
        std::fs::rename(live_path, backup_path).with_context(|| {
            format!(
                "failed to backup {} to {}",
                live_path.display(),
                backup_path.display()
            )
        })?;
    }

    if let Err(error) = std::fs::rename(staged_path, live_path) {
        if had_live_file {
            drop(std::fs::rename(backup_path, live_path));
        }
        return Err(error).with_context(|| format!("failed to activate {}", live_path.display()));
    }

    Ok(())
}

fn restore_live_file(live_path: &Path, backup_path: &Path, had_live_file: bool) -> Result<()> {
    remove_file_if_exists(live_path)?;

    if had_live_file {
        std::fs::rename(backup_path, live_path).with_context(|| {
            format!(
                "failed to restore {} from {}",
                live_path.display(),
                backup_path.display()
            )
        })?;
    }

    Ok(())
}

fn remove_file_if_exists(path: &Path) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }

    std::fs::remove_file(path).with_context(|| format!("failed to remove {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::{CaddyProcessController, apply_caddy_state_with_controller};
    use crate::serve::CaddyState;
    use anyhow::{Result, anyhow};
    use std::collections::BTreeMap;
    use std::path::Path;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct MockController {
        reload_attempts: AtomicUsize,
        observed_configs: Mutex<Vec<String>>,
        fail_first_reload: bool,
    }

    impl MockController {
        fn succeeds() -> Self {
            Self {
                reload_attempts: AtomicUsize::new(0),
                observed_configs: Mutex::new(Vec::new()),
                fail_first_reload: false,
            }
        }

        fn fails_once_then_succeeds() -> Self {
            Self {
                reload_attempts: AtomicUsize::new(0),
                observed_configs: Mutex::new(Vec::new()),
                fail_first_reload: true,
            }
        }
    }

    impl CaddyProcessController for MockController {
        fn ensure_installed(&self) -> Result<()> {
            Ok(())
        }

        fn validate_config(&self, caddyfile_path: &Path) -> Result<()> {
            let content = std::fs::read_to_string(caddyfile_path).expect("read staged caddyfile");
            self.observed_configs
                .lock()
                .expect("lock configs")
                .push(format!("validate:{content}"));
            Ok(())
        }

        fn reload_or_start(&self, caddyfile_path: &Path) -> Result<()> {
            let content = std::fs::read_to_string(caddyfile_path).expect("read active caddyfile");
            self.observed_configs
                .lock()
                .expect("lock configs")
                .push(format!("reload:{content}"));

            let attempt = self.reload_attempts.fetch_add(1, Ordering::SeqCst);
            if self.fail_first_reload && attempt == 0 {
                return Err(anyhow!("reload failed"));
            }

            Ok(())
        }
    }

    fn temp_caddy_dir(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "berth-caddy-apply-{name}-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ))
    }

    fn state(route: &str, upstream: &str) -> CaddyState {
        let mut routes = BTreeMap::new();
        routes.insert(route.to_owned(), upstream.to_owned());
        CaddyState { routes }
    }

    #[test]
    fn apply_caddy_state_persists_new_files_and_cleans_artifacts_on_success() {
        let caddy_dir = temp_caddy_dir("success");
        std::fs::create_dir_all(&caddy_dir).expect("create caddy dir");
        std::fs::write(
            caddy_dir.join("sites.toml"),
            "[routes]\nold = \"127.0.0.1:1\"\n",
        )
        .expect("seed state");
        std::fs::write(caddy_dir.join("Caddyfile"), "old config").expect("seed caddyfile");

        let controller = MockController::succeeds();
        let next_state = state("shipit-api.berth", "127.0.0.1:8080");
        apply_caddy_state_with_controller(&caddy_dir, &next_state, "new config", &controller)
            .expect("apply caddy state");

        assert!(
            std::fs::read_to_string(caddy_dir.join("sites.toml"))
                .expect("read state")
                .contains("shipit-api.berth")
        );
        assert_eq!(
            std::fs::read_to_string(caddy_dir.join("Caddyfile")).expect("read caddyfile"),
            "new config"
        );
        assert!(!caddy_dir.join("sites.toml.bak").exists());
        assert!(!caddy_dir.join("Caddyfile.bak").exists());
        assert!(!caddy_dir.join("sites.toml.staged").exists());
        assert!(!caddy_dir.join("Caddyfile.staged").exists());
    }

    #[test]
    fn apply_caddy_state_rolls_back_live_files_when_reload_fails() {
        let caddy_dir = temp_caddy_dir("rollback");
        std::fs::create_dir_all(&caddy_dir).expect("create caddy dir");
        let previous_state = "[routes]\nshipit-bill.berth = \"127.0.0.1:3000\"\n";
        std::fs::write(caddy_dir.join("sites.toml"), previous_state).expect("seed state");
        std::fs::write(caddy_dir.join("Caddyfile"), "old config").expect("seed caddyfile");

        let controller = MockController::fails_once_then_succeeds();
        let next_state = state("shipit-api.berth", "127.0.0.1:8080");
        let error =
            apply_caddy_state_with_controller(&caddy_dir, &next_state, "new config", &controller)
                .expect_err("reload should fail");

        assert!(error.to_string().contains("failed to apply caddy config"));
        assert_eq!(
            std::fs::read_to_string(caddy_dir.join("sites.toml")).expect("read state"),
            previous_state
        );
        assert_eq!(
            std::fs::read_to_string(caddy_dir.join("Caddyfile")).expect("read caddyfile"),
            "old config"
        );
        let observed = controller.observed_configs.lock().expect("lock observed");
        assert_eq!(
            observed.as_slice(),
            [
                "validate:new config",
                "reload:new config",
                "reload:old config"
            ]
        );
    }

    #[test]
    fn apply_caddy_state_recovers_interrupted_backup_before_applying() {
        let caddy_dir = temp_caddy_dir("recover");
        std::fs::create_dir_all(&caddy_dir).expect("create caddy dir");
        std::fs::write(
            caddy_dir.join("sites.toml.bak"),
            "[routes]\nshipit-track.berth = \"127.0.0.1:4000\"\n",
        )
        .expect("seed backup state");
        std::fs::write(caddy_dir.join("Caddyfile.bak"), "restorable config")
            .expect("seed backup caddyfile");

        let controller = MockController::succeeds();
        let next_state = state("shipit-postal.berth", "127.0.0.1:5000");
        apply_caddy_state_with_controller(&caddy_dir, &next_state, "next config", &controller)
            .expect("apply recovered state");

        assert!(
            std::fs::read_to_string(caddy_dir.join("sites.toml"))
                .expect("read state")
                .contains("shipit-postal.berth")
        );
        assert_eq!(
            std::fs::read_to_string(caddy_dir.join("Caddyfile")).expect("read caddyfile"),
            "next config"
        );
        assert!(!caddy_dir.join("sites.toml.bak").exists());
        assert!(!caddy_dir.join("Caddyfile.bak").exists());
    }
}
