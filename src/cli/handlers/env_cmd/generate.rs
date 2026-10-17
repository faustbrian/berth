//! cli handlers env cmd generate module.
//!
//! Writes the managed environment to a new file the caller names. It never
//! replaces an existing file (a project `.env` included): `create_new` fails
//! on any existing path, a symbolic link included, so the write can never be
//! redirected to a file that already exists.

use anyhow::{Context, Result};
use std::collections::BTreeMap;
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

use crate::cli::handlers::log;
use crate::env;

/// Handles the `env generate` CLI command.
///
/// # Errors
///
/// Returns an error when the file exists, its directory is a symbolic link,
/// or it cannot be written.
pub(super) fn handle_generate_env(
    output: &Path,
    values: &BTreeMap<String, String>,
    quiet: bool,
) -> Result<()> {
    if crate::docker::is_dry_run() {
        log::info_if_not_quiet(
            quiet,
            "env",
            &format!("[dry-run] Would write {}", output.display()),
        );
        return Ok(());
    }
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(output)
        .map_err(|error| {
            if error.kind() == std::io::ErrorKind::AlreadyExists {
                anyhow::anyhow!(
                    "{} already exists; berth never replaces an env file, choose a new path",
                    output.display()
                )
            } else {
                anyhow::Error::new(error).context(format!("failed to create {}", output.display()))
            }
        })?;
    file.write_all(env::render_dotenv(values).as_bytes())
        .and_then(|()| file.sync_all())
        .with_context(|| format!("failed to write {}", output.display()))?;
    log::info_if_not_quiet(
        quiet,
        "env",
        &format!("Generated {} with {} keys", output.display(), values.len()),
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::handle_generate_env;
    use std::collections::BTreeMap;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn generates_a_new_private_file_and_never_replaces_one() {
        let dir = std::env::temp_dir().join(format!("berth-env-generate-{}", std::process::id()));
        drop(std::fs::remove_dir_all(&dir));
        std::fs::create_dir_all(&dir).expect("dir");
        let path = dir.join("env.berth");
        let values = BTreeMap::from([("DB_HOST".to_owned(), "db".to_owned())]);
        handle_generate_env(&path, &values, true).expect("generate");
        assert_eq!(
            std::fs::read_to_string(&path).expect("read"),
            "DB_HOST=\"db\"\n"
        );
        let mode = std::fs::metadata(&path).expect("meta").permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);

        let existing = dir.join(".env");
        std::fs::write(&existing, "APP_NAME=mine\n").expect("existing");
        assert!(handle_generate_env(&existing, &values, true).is_err());
        assert_eq!(
            std::fs::read_to_string(&existing).expect("read"),
            "APP_NAME=mine\n"
        );

        let target = dir.join("victim");
        std::fs::write(&target, "KEEP=1\n").expect("victim");
        let link = dir.join("link.env");
        std::os::unix::fs::symlink(&target, &link).expect("link");
        assert!(
            handle_generate_env(&link, &values, true).is_err(),
            "a symlink is never followed"
        );
        assert_eq!(std::fs::read_to_string(&target).expect("read"), "KEEP=1\n");
        std::fs::remove_dir_all(&dir).expect("cleanup");
    }
}
