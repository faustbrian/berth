//! Strict YAML reading and writing for berth project files and lockfiles.
//!
//! Project files come from worktrees that berth does not control, so parsing
//! first walks the document's events (see [`events`]) and rejects anchors,
//! aliases, tags, merge keys, duplicate keys, multiple documents, and
//! documents deeper than 32 levels or larger than 10,000 nodes. Only then is
//! the document deserialized, with YAML 1.1 boolean words (`yes`, `off`)
//! rejected rather than coerced.

mod events;

pub(crate) use events::parse_text_tree;

use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_saphyr::options::{AliasLimits, DuplicateKeyPolicy, MergeKeyPolicy, Options};

/// Largest project file or lockfile berth reads, in bytes.
pub(crate) const MAX_YAML_FILE_BYTES: u64 = 1024 * 1024;

fn strict_options() -> Options {
    let mut alias_limits = AliasLimits::default();
    alias_limits.max_total_replayed_events = 0;
    alias_limits.max_replay_stack_depth = 0;
    alias_limits.max_alias_expansions_per_anchor = 0;

    let mut options = Options::default();
    options.duplicate_keys = DuplicateKeyPolicy::Error;
    options.merge_keys = MergeKeyPolicy::Error;
    options.alias_limits = alias_limits;
    options.reject_unsupported_tags = true;
    options.strict_booleans = true;
    options
}

/// Deserializes one strict YAML document.
///
/// # Errors
///
/// Returns the parser error, which names the offending line and column.
pub(crate) fn from_str<T: DeserializeOwned>(content: &str) -> anyhow::Result<T> {
    parse_text_tree(content)?;
    serde_saphyr::from_str_with_options(content, strict_options()).map_err(anyhow::Error::from)
}

/// Serializes a value as YAML.
///
/// # Errors
///
/// Returns an error when the value cannot be represented as YAML.
pub(crate) fn to_string<T: Serialize>(value: &T) -> anyhow::Result<String> {
    serde_saphyr::to_string(value).map_err(anyhow::Error::from)
}

/// Reads a regular, non-symlink file of at most [`MAX_YAML_FILE_BYTES`].
///
/// The path is checked with `lstat` before opening, and the opened handle must
/// be the same regular file (device and inode), so a path swapped for a
/// symbolic link between the check and the open is refused.
///
/// # Errors
///
/// Returns an error for symbolic links, non-regular files, oversized files
/// and read failures.
pub(crate) fn read_bounded_file(path: &std::path::Path) -> anyhow::Result<String> {
    use anyhow::Context;
    use std::io::Read;
    use std::os::unix::fs::MetadataExt;

    let before = std::fs::symlink_metadata(path)
        .with_context(|| format!("failed to inspect {}", path.display()))?;
    if before.file_type().is_symlink() {
        anyhow::bail!(
            "{} is a symbolic link; berth reads only regular files",
            path.display()
        );
    }
    if !before.is_file() {
        anyhow::bail!("{} is not a regular file", path.display());
    }
    let file =
        std::fs::File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    let opened = file
        .metadata()
        .with_context(|| format!("failed to inspect {}", path.display()))?;
    if !opened.is_file() || opened.dev() != before.dev() || opened.ino() != before.ino() {
        anyhow::bail!("{} changed while it was being opened", path.display());
    }
    if opened.len() > MAX_YAML_FILE_BYTES {
        anyhow::bail!(
            "{} is {} bytes; the maximum is {MAX_YAML_FILE_BYTES}",
            path.display(),
            opened.len()
        );
    }
    let mut content = String::new();
    file.take(MAX_YAML_FILE_BYTES + 1)
        .read_to_string(&mut content)
        .with_context(|| format!("failed to read {}", path.display()))?;
    if content.len() as u64 > MAX_YAML_FILE_BYTES {
        anyhow::bail!(
            "{} grew beyond {MAX_YAML_FILE_BYTES} bytes while reading",
            path.display()
        );
    }
    Ok(content)
}

/// Writes `content` to `path` atomically, refusing to follow a symbolic link
/// at the destination.
///
/// # Errors
///
/// Returns an error when the destination is a symbolic link or not a regular
/// file, or when writing or renaming fails.
pub(crate) fn write_file_atomically(path: &std::path::Path, content: &str) -> anyhow::Result<()> {
    use anyhow::Context;
    use std::io::Write;

    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            anyhow::bail!(
                "{} is a symbolic link; berth will not write through it",
                path.display()
            )
        }
        Ok(metadata) if !metadata.is_file() => {
            anyhow::bail!("{} is not a regular file", path.display())
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(error).with_context(|| format!("failed to inspect {}", path.display()));
        }
    }
    let name = path
        .file_name()
        .and_then(std::ffi::OsStr::to_str)
        .ok_or_else(|| anyhow::anyhow!("{} has no file name", path.display()))?;
    let staging = path.with_file_name(format!(".{name}.berth-write-{}", std::process::id()));
    let result = (|| -> anyhow::Result<()> {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&staging)
            .with_context(|| format!("failed to create {}", staging.display()))?;
        file.write_all(content.as_bytes())
            .and_then(|()| file.sync_all())
            .with_context(|| format!("failed to write {}", staging.display()))?;
        std::fs::rename(&staging, path)
            .with_context(|| format!("failed to replace {}", path.display()))
    })();
    if result.is_err() {
        drop(std::fs::remove_file(&staging));
    }
    result
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::{MAX_YAML_FILE_BYTES, from_str, read_bounded_file};

    type Mapping = BTreeMap<String, serde_json::Value>;

    #[test]
    fn rejects_aliases_merge_keys_and_duplicate_keys() {
        for (document, reason) in [
            ("base: &base\n  A: \"1\"\ncopy: *base\n", "anchors"),
            (
                "base:\n  A: \"1\"\nchild:\n  <<: {B: \"2\"}\n",
                "merge keys",
            ),
            ("a:\n  X: \"1\"\na:\n  X: \"2\"\n", "duplicate key"),
            ("a: !custom x\n", "tags"),
        ] {
            let error = from_str::<Mapping>(document).expect_err(document);
            assert!(
                format!("{error:#}").contains(reason),
                "{document}: {error:#}"
            );
        }
    }

    #[test]
    fn rejects_multiple_documents_and_yaml_1_1_booleans() {
        let documents = from_str::<Mapping>("a: 1\n---\nb: 2\n").expect_err("two documents");
        assert!(
            format!("{documents:#}").contains("exactly one"),
            "{documents:#}"
        );

        #[derive(serde::Deserialize)]
        struct Flag {
            #[allow(dead_code, reason = "only deserialization is exercised")]
            octane: bool,
        }
        assert!(
            from_str::<Flag>("octane: yes\n").is_err(),
            "yes is not a boolean"
        );
        assert!(from_str::<Flag>("octane: true\n").is_ok());
    }

    #[test]
    fn bounded_reads_refuse_symlinks_and_oversized_files() -> anyhow::Result<()> {
        let dir = crate::test_support::temp_root()
            .join(format!("berth-yaml-bounds-{}", std::process::id()));
        drop(std::fs::remove_dir_all(&dir));
        std::fs::create_dir_all(&dir)?;
        let real = dir.join("real.yaml");
        std::fs::write(&real, "a: b\n")?;
        let link = dir.join("link.yaml");
        std::os::unix::fs::symlink(&real, &link)?;
        let large = dir.join("large.yaml");
        std::fs::write(
            &large,
            vec![b'#'; usize::try_from(MAX_YAML_FILE_BYTES)? + 1],
        )?;

        assert!(read_bounded_file(&real).is_ok());
        assert!(
            format!("{:#}", read_bounded_file(&link).expect_err("symlink"))
                .contains("symbolic link")
        );
        assert!(
            format!("{:#}", read_bounded_file(&large).expect_err("oversized")).contains("maximum")
        );
        std::fs::remove_dir_all(&dir)?;
        Ok(())
    }

    #[test]
    fn atomic_writes_refuse_a_symlinked_destination() -> anyhow::Result<()> {
        let dir = crate::test_support::temp_root()
            .join(format!("berth-yaml-write-{}", std::process::id()));
        drop(std::fs::remove_dir_all(&dir));
        std::fs::create_dir_all(&dir)?;
        let victim = dir.join("victim.txt");
        std::fs::write(&victim, "keep")?;
        let lock = dir.join(".berth.lock.yaml");
        std::os::unix::fs::symlink(&victim, &lock)?;

        let error = super::write_file_atomically(&lock, "images: []\n").expect_err("symlink");
        assert!(format!("{error:#}").contains("symbolic link"), "{error:#}");
        assert_eq!(std::fs::read_to_string(&victim)?, "keep");

        let plain = dir.join("plain.yaml");
        super::write_file_atomically(&plain, "a: b\n")?;
        assert_eq!(std::fs::read_to_string(&plain)?, "a: b\n");
        std::fs::remove_dir_all(&dir)?;
        Ok(())
    }
}
