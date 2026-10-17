//! Signature and tag derivation for serve derived images.

use std::hash::{Hash, Hasher};

/// Derives a docker-safe derived image tag from container name and signature.
pub(super) fn derived_image_tag(container_name: &str, signature: &str) -> String {
    let sanitized: String = container_name
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' || ch == '.' {
                ch
            } else {
                '-'
            }
        })
        .collect();
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    signature.hash(&mut hasher);
    let digest = hasher.finish();
    format!("berth/{}-serve-{:x}", sanitized.to_lowercase(), digest)
}

/// Derives the cache signature describing derived image contents.
pub(super) fn derive_image_signature(dockerfile: &str) -> String {
    format!(
        "dockerfile-fnv1a64-v1:{:016x}",
        fnv1a64(dockerfile.as_bytes())
    )
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    const OFFSET_BASIS: u64 = 0xcbf29ce484222325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;

    let mut hash = OFFSET_BASIS;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(PRIME);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::derive_image_signature;

    #[test]
    fn image_signature_is_stable_for_identical_dockerfile_content() {
        let dockerfile = "FROM base\nRUN echo hello\n";
        let first = derive_image_signature(dockerfile);
        let second = derive_image_signature(dockerfile);
        assert_eq!(first, second);
    }

    #[test]
    fn image_signature_changes_when_dockerfile_content_changes() {
        let first = derive_image_signature("FROM base\nRUN echo hello\n");
        let second = derive_image_signature("FROM base\nRUN echo world\n");
        assert_ne!(first, second);
    }
}
