//! The managed Laravel `APP_KEY` of an instance.
//!
//! A fresh worktree has no `.env` (it is gitignored) and berth never writes
//! one, so berth injects a random per-instance key instead. It lives under
//! the berth home, stays stable until the instance is purged, and is never
//! derived from anything visible in Docker labels.

use anyhow::Result;
use std::path::Path;

const FILE_NAME: &str = "app-key";

/// Returns the instance's managed `APP_KEY` (`base64:` + 32 random bytes),
/// creating it on first use.
///
/// # Errors
///
/// Returns an error when the key file cannot be read or created, or holds an
/// invalid value.
pub(crate) fn app_key(project: &str, instance: &str) -> Result<String> {
    if cfg!(test) {
        return Ok(format!("base64:{}=", "A".repeat(43)));
    }
    super::secret_file::read_or_create(
        &Path::new("instances").join(project).join(instance),
        FILE_NAME,
        random_key,
        is_valid,
    )
    .map_err(|error| {
        super::advise_on_invalid(error, || {
            "the file is the instance's APP_KEY: restore it from a backup, or delete it so berth \
             creates a new one (sessions and data encrypted with the old key become unreadable)"
                .to_owned()
        })
    })
}

fn is_valid(key: &str) -> bool {
    key.strip_prefix("base64:").is_some_and(|encoded| {
        encoded.len() == 44
            && encoded
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'/' | b'='))
    })
}

fn random_key() -> Result<String> {
    let mut bytes = [0_u8; 32];
    getrandom::fill(&mut bytes).map_err(|error| anyhow::anyhow!("no OS randomness: {error}"))?;
    Ok(format!("base64:{}", base64_encode(&bytes)))
}

/// Standard base64 with padding (RFC 4648), as Laravel's `key:generate`.
fn base64_encode(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = u32::from(b[0]) << 16 | u32::from(b[1]) << 8 | u32::from(b[2]);
        for (index, shift) in [18, 12, 6, 0].into_iter().enumerate() {
            if index <= chunk.len() {
                out.push(char::from(ALPHABET[((n >> shift) & 63) as usize]));
            } else {
                out.push('=');
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{base64_encode, is_valid};

    #[test]
    fn keys_are_padded_base64_of_32_bytes_like_key_generate() {
        assert_eq!(base64_encode(b"Man"), "TWFu");
        assert_eq!(base64_encode(b"Ma"), "TWE=");
        assert_eq!(base64_encode(b"M"), "TQ==");
        let key = format!("base64:{}", base64_encode(&[7_u8; 32]));
        assert_eq!(key.len(), 51);
        assert!(is_valid(&key), "{key}");
        assert!(!is_valid("base64:short"));
    }
}
