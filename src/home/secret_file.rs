//! Secrets berth generates once and keeps under the berth home.

use anyhow::{Context, Result};
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

/// Returns the secret in `~/.berth/{dir}/{file}`, creating it with
/// `generate` on first use. The file is staged and hard-linked into place,
/// so concurrent first uses agree on one value and a crash leaves no
/// partial secret; it is mode 0600 in a 0700 directory.
///
/// # Errors
///
/// Returns an error when the file cannot be read or created, or `valid`
/// rejects its contents.
pub(crate) fn read_or_create(
    dir: &Path,
    file: &str,
    generate: impl FnOnce() -> Result<String>,
    valid: impl Fn(&str) -> bool,
) -> Result<String> {
    let dir = super::private_dir(dir)?;
    read_or_create_in(&dir, file, generate, valid)
}

/// [`read_or_create`] inside an existing private directory.
fn read_or_create_in(
    dir: &Path,
    file: &str,
    generate: impl FnOnce() -> Result<String>,
    valid: impl Fn(&str) -> bool,
) -> Result<String> {
    let path = dir.join(file);
    if path.exists() {
        return read(&path, &valid);
    }
    let secret = generate()?;
    // Unique per process and per call: threads of one berth process (parallel
    // service startup) may create the same secret at once.
    static STAGED: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let staged = dir.join(format!(
        ".{file}.{}.{}",
        std::process::id(),
        STAGED.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    let result = stage(&staged, &secret).and_then(|()| match std::fs::hard_link(&staged, &path) {
        Ok(()) => Ok(secret),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => read(&path, &valid),
        Err(error) => Err(error).with_context(|| format!("failed to create {}", path.display())),
    });
    drop(std::fs::remove_file(&staged));
    result
}

/// 32 bytes of OS randomness as 64 lowercase hex characters.
///
/// # Errors
///
/// Returns an error when the OS has no randomness to offer.
pub(crate) fn random_hex_secret() -> Result<String> {
    let mut bytes = [0_u8; 32];
    getrandom::fill(&mut bytes).map_err(|error| anyhow::anyhow!("no OS randomness: {error}"))?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

/// Whether `value` is 64 lowercase hex characters.
#[must_use]
pub(crate) fn is_hex_secret(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
}

fn stage(path: &Path, secret: &str) -> Result<()> {
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .with_context(|| format!("failed to create {}", path.display()))?;
    file.write_all(secret.as_bytes())
        .and_then(|()| file.sync_all())
        .with_context(|| format!("failed to write {}", path.display()))
}

fn read(path: &Path, valid: &impl Fn(&str) -> bool) -> Result<String> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    let secret = raw.trim().to_owned();
    if !valid(&secret) {
        anyhow::bail!(
            "{} does not hold a valid secret; delete it to create a new one",
            path.display()
        );
    }
    Ok(secret)
}

#[cfg(test)]
mod tests {
    use super::{is_hex_secret, random_hex_secret, read_or_create_in};
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn a_secret_is_private_and_concurrent_first_uses_agree() {
        let dir = std::env::temp_dir().join(format!("berth-secret-file-{}", std::process::id()));
        drop(std::fs::remove_dir_all(&dir));
        std::fs::create_dir_all(&dir).expect("dir");
        let values: Vec<String> = std::thread::scope(|scope| {
            let handles: Vec<_> = (0..16)
                .map(|_| {
                    scope.spawn(|| {
                        read_or_create_in(&dir, "admin", random_hex_secret, is_hex_secret)
                            .expect("secret")
                    })
                })
                .collect();
            handles
                .into_iter()
                .map(|handle| handle.join().expect("thread"))
                .collect()
        });
        assert!(
            values.windows(2).all(|pair| pair[0] == pair[1]),
            "{values:?}"
        );
        let file_mode = std::fs::metadata(dir.join("admin"))
            .expect("file")
            .permissions()
            .mode();
        // The 0700 directory comes from `home::private_dir` (its own tests).
        assert_eq!(file_mode & 0o777, 0o600);
        let leftovers = std::fs::read_dir(&dir).expect("dir").count();
        assert_eq!(leftovers, 1, "no staging file is left behind");
        // A corrupted secret fails loudly instead of being replaced.
        std::fs::write(dir.join("admin"), "not hex").expect("corrupt");
        let error = read_or_create_in(&dir, "admin", random_hex_secret, is_hex_secret)
            .expect_err("corrupt secret");
        assert!(error.to_string().contains("delete it"), "{error}");
        std::fs::remove_dir_all(&dir).expect("cleanup");
    }

    #[test]
    fn secrets_are_256_bit_hex() {
        let secret = random_hex_secret().expect("secret");
        assert!(is_hex_secret(&secret), "{secret}");
        assert_ne!(secret, random_hex_secret().expect("secret"));
        assert!(!is_hex_secret("ABC"));
    }
}
