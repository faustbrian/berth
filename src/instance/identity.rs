//! Instance identity: one environment of a project per checkout.
//!
//! The ID is derived from the canonical project root so every worktree of a
//! repository gets its own environment without editing committed config, and
//! the same checkout always maps to the same environment.

use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

/// Longest instance ID; keeps `{instance}-{project}-{service}` within one
/// 63-byte DNS label together with the project and service name limits.
pub(crate) const MAX_INSTANCE_ID_LEN: usize = 23;

/// Longest readable slug before the `-{hash6}` suffix.
const MAX_SLUG_LEN: usize = 16;

/// The resolved identity of one berth environment.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct InstanceIdentity {
    /// Instance ID such as `agent-7-3fa29c`.
    pub(crate) id: String,
    /// Canonical project root the instance belongs to.
    pub(crate) root: PathBuf,
}

/// Resolves the instance for a project root.
///
/// `requested` comes from `--instance` or `BERTH_INSTANCE`; it is validated
/// as written and never normalized. Without it, an instance bound to this
/// root earlier in the process (see `requested`) is used, else the ID is
/// derived from the root.
///
/// # Errors
///
/// Returns an error when the root cannot be canonicalized or the requested ID
/// is not a valid instance ID.
pub(crate) fn resolve_instance(
    project_root: &Path,
    requested: Option<&str>,
) -> Result<InstanceIdentity> {
    let root = project_root
        .canonicalize()
        .with_context(|| format!("failed to resolve project root {}", project_root.display()))?;
    let id = match requested {
        Some(requested) => {
            validate_instance_id(requested)?;
            super::bind_requested_instance(&root, requested);
            requested.to_owned()
        }
        None => super::requested_instance_for(&root).unwrap_or_else(|| derive_instance_id(&root)),
    };
    Ok(InstanceIdentity { id, root })
}

/// Derives `{slug}-{hash6}` from a canonical root path.
#[must_use]
pub(crate) fn derive_instance_id(canonical_root: &Path) -> String {
    let basename = canonical_root
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let digest = Sha256::digest(canonical_root.as_os_str().as_encoded_bytes());
    let hash = digest
        .iter()
        .take(3)
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    format!("{}-{hash}", slug(&basename))
}

/// Validates an explicitly requested instance ID.
///
/// # Errors
///
/// Returns an error naming the exact rejected input.
pub(crate) fn validate_instance_id(id: &str) -> Result<()> {
    let bytes = id.as_bytes();
    let valid_chars = bytes
        .iter()
        .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'-');
    let valid_edges = bytes.first().is_some_and(u8::is_ascii_alphanumeric)
        && bytes.last().is_some_and(u8::is_ascii_alphanumeric);
    if !valid_chars || !valid_edges {
        anyhow::bail!(
            "instance '{id}' (from --instance or BERTH_INSTANCE) must be lowercase letters, \
             digits and inner hyphens, at most {MAX_INSTANCE_ID_LEN} bytes"
        );
    }
    if id.len() > MAX_INSTANCE_ID_LEN {
        anyhow::bail!(
            "instance '{id}' (from --instance or BERTH_INSTANCE) is {} bytes; the maximum is \
             {MAX_INSTANCE_ID_LEN}",
            id.len()
        );
    }
    Ok(())
}

/// Lowercases ASCII alphanumerics and turns every other run of characters
/// into one hyphen, bounded to [`MAX_SLUG_LEN`]. An empty result becomes `i`
/// so the ID always starts with a letter or digit; uniqueness comes from the
/// hash suffix, never from the slug.
fn slug(name: &str) -> String {
    let mut slug = String::with_capacity(name.len().min(MAX_SLUG_LEN));
    for character in name.chars() {
        if character.is_ascii_alphanumeric() {
            slug.push(character.to_ascii_lowercase());
        } else if !slug.is_empty() && !slug.ends_with('-') {
            slug.push('-');
        }
        if slug.len() >= MAX_SLUG_LEN {
            break;
        }
    }
    let slug = slug.trim_end_matches('-');
    if slug.is_empty() {
        "i".to_owned()
    } else {
        slug.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{MAX_INSTANCE_ID_LEN, derive_instance_id, resolve_instance, validate_instance_id};

    #[test]
    fn derivation_is_stable_for_a_fixed_path() {
        assert_eq!(
            derive_instance_id(Path::new("/work/shipit-api")),
            derive_instance_id(Path::new("/work/shipit-api"))
        );
        // Golden value: changing the hash or slug rules would rename every
        // existing environment.
        assert_eq!(
            derive_instance_id(Path::new("/work/shipit-api")),
            "shipit-api-b3886b"
        );
    }

    #[test]
    fn hostile_directory_names_yield_bounded_valid_ids() {
        for name in [
            ".agent-1",
            "føø",
            "日本語",
            "Agent_Worktree__42",
            "a-very-long-worktree-directory-name-that-goes-on-and-on",
            "--x--",
            "",
        ] {
            let path = Path::new("/work").join(name);
            let id = derive_instance_id(&path);
            assert!(
                validate_instance_id(&id).is_ok(),
                "{name:?} gave invalid id {id}"
            );
            assert!(id.len() <= MAX_INSTANCE_ID_LEN, "{name:?} gave {id}");
        }
    }

    #[test]
    fn same_basename_in_different_directories_gets_different_ids() {
        assert_ne!(
            derive_instance_id(Path::new("/one/api")),
            derive_instance_id(Path::new("/two/api"))
        );
    }

    #[test]
    fn requested_ids_are_validated_as_written() {
        assert!(validate_instance_id("agent-7").is_ok());
        for invalid in [
            "Agent",
            "agent_7",
            "-x",
            "x-",
            "",
            "a-b-c-d-e-f-g-h-i-j-k-lm",
        ] {
            let error = validate_instance_id(invalid).expect_err(invalid);
            assert!(
                error.to_string().contains(&format!("'{invalid}'")),
                "{error}"
            );
        }
    }

    #[test]
    fn symlinked_and_real_roots_resolve_to_the_same_instance() -> anyhow::Result<()> {
        let base =
            crate::test_support::temp_root().join(format!("berth-instance-{}", std::process::id()));
        drop(std::fs::remove_dir_all(&base));
        let real = base.join("real-checkout");
        std::fs::create_dir_all(real.join("nested"))?;
        let link = base.join("link-checkout");
        std::os::unix::fs::symlink(&real, &link)?;

        let from_real = resolve_instance(&real, None)?;
        let from_link = resolve_instance(&link, None)?;
        assert_eq!(from_real, from_link);
        std::fs::remove_dir_all(&base)?;
        Ok(())
    }
}
