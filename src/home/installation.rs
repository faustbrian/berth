//! The random ID that identifies this berth installation.
//!
//! Every Docker object berth creates is labeled with it, so cleanup and
//! reaping only ever touch objects this installation created (an isolated
//! `BERTH_HOME` is a separate installation).

use anyhow::{Context, Result};
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

const FILE_NAME: &str = "installation-id";

/// Returns the installation ID, creating it on first use.
///
/// # Errors
///
/// Returns an error when the ID file cannot be read or created, or holds an
/// invalid value.
pub(crate) fn installation_id() -> Result<String> {
    // Unit tests never touch the real berth home; the binary's integration
    // tests set `BERTH_HOME` instead.
    if cfg!(test) {
        return Ok("0".repeat(32));
    }
    let dir = super::private_dir(Path::new(""))?;
    create_or_read(&dir.join(FILE_NAME))
}

/// Returns the installation ID if one exists, without creating it: for dry
/// runs, which write nothing. Without one, no container is this
/// installation's yet.
///
/// # Errors
///
/// Returns an error when the ID file exists but cannot be read or holds an
/// invalid value.
pub(crate) fn existing_installation_id() -> Result<Option<String>> {
    if cfg!(test) {
        return Ok(Some("0".repeat(32)));
    }
    let path = super::home_path(Path::new(FILE_NAME))?;
    if !path.exists() {
        return Ok(None);
    }
    read_id(&path).map(Some)
}

/// Writes a complete ID to a private temporary file and hard-links it into
/// place. `link` never replaces an existing file, so the first run wins
/// atomically and a crash can never leave a partial ID behind.
fn create_or_read(path: &Path) -> Result<String> {
    if path.exists() {
        return read_id(path);
    }
    let id = random_id()?;
    let staged = path.with_file_name(format!(".{FILE_NAME}.{}.{id}", std::process::id()));
    let result = stage_id(&staged, &id).and_then(|()| match std::fs::hard_link(&staged, path) {
        Ok(()) => Ok(id),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => read_id(path),
        Err(error) => Err(error).with_context(|| format!("failed to create {}", path.display())),
    });
    drop(std::fs::remove_file(&staged));
    result
}

fn stage_id(staged: &Path, id: &str) -> Result<()> {
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(staged)
        .with_context(|| format!("failed to create {}", staged.display()))?;
    file.write_all(id.as_bytes())
        .and_then(|()| file.sync_all())
        .with_context(|| format!("failed to write {}", staged.display()))
}

fn read_id(path: &Path) -> Result<String> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    // A hand-restored ID often ends in a newline; the ID itself never does.
    let id = raw.trim().to_owned();
    if id.len() != 32 || !id.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        // A fresh ID would make every object labelled with the old one
        // another installation's, which berth then refuses to touch.
        let docker = crate::docker::docker_command();
        let label = crate::docker::label_template("dev.berth.installation");
        anyhow::bail!(
            "{} does not contain a valid installation ID (32 hex characters). Do not delete it: \
             every container, volume and network berth created carries that ID, and a new one \
             orphans them. Recover it from a label: `{docker} ps -a --filter \
             label=dev.berth.installation --format '{label} {{{{.Names}}}}'` lists each container \
             with its ID; pick the one whose containers belong to this berth home (their names \
             are berth_<project>_<instance>_<service>, matching {}), and write it back with \
             `printf %s <id> > {}`",
            path.display(),
            path.with_file_name("instances").display(),
            crate::followup::quote(&path.display().to_string())
        );
    }
    Ok(id)
}

fn random_id() -> Result<String> {
    let mut bytes = [0_u8; 16];
    getrandom::fill(&mut bytes).map_err(|error| anyhow::anyhow!("no OS randomness: {error}"))?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_corrupt_id_names_recovery_commands_for_the_active_engine() {
        let home = crate::test_support::temp_root()
            .join(format!("berth-installation-corrupt-{}", std::process::id()));
        drop(std::fs::remove_dir_all(&home));
        std::fs::create_dir_all(&home).expect("home");
        let path = home.join(super::FILE_NAME);
        std::fs::write(&path, "not an id").expect("corrupt");
        let message =
            crate::docker::with_container_engine(crate::config::ContainerEngine::Podman, || {
                format!("{:#}", super::read_id(&path).expect_err("corrupt"))
            });
        assert!(message.contains("Do not delete it"), "{message}");
        assert!(
            message.contains("{{index .Labels \"dev.berth.installation\"}}"),
            "Podman has no `.Label`: {message}"
        );
        // A restored ID with a trailing newline is accepted.
        std::fs::write(&path, format!("{}\n", "a".repeat(32))).expect("restored");
        assert_eq!(super::read_id(&path).expect("id"), "a".repeat(32));
        std::fs::remove_dir_all(&home).expect("cleanup");
    }

    #[test]
    fn concurrent_first_runs_agree_on_one_installation_id() {
        let home = crate::test_support::temp_root()
            .join(format!("berth-installation-{}", std::process::id()));
        drop(std::fs::remove_dir_all(&home));
        std::fs::create_dir_all(&home).expect("home");
        let path = home.join(super::FILE_NAME);
        let ids: Vec<String> = (0..8)
            .map(|_| {
                let path = path.clone();
                std::thread::spawn(move || super::create_or_read(&path).expect("id"))
            })
            .collect::<Vec<_>>()
            .into_iter()
            .map(|handle| handle.join().expect("thread"))
            .collect();
        assert!(ids.windows(2).all(|pair| pair[0] == pair[1]), "{ids:?}");
        assert_eq!(ids[0].len(), 32);
        std::fs::remove_dir_all(&home).expect("cleanup");
    }
}
