//! Project-level app domain naming helpers.
//!
//! Contains shared domain name generation logic used by config workflows.

use std::path::Path;

use super::DomainStrategy;

/// Returns the default project slug for config-derived naming.
#[must_use]
pub(crate) fn sanitize_project_slug(name: &str) -> String {
    let mut slug = String::new();
    let mut previous_dash = false;

    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
            previous_dash = false;
            continue;
        }

        if !previous_dash {
            slug.push('-');
            previous_dash = true;
        }
    }

    let slug = slug.trim_matches('-');
    if slug.is_empty() {
        "my-app".to_owned()
    } else {
        slug.to_owned()
    }
}

/// Resolves the generated base label for a project root and strategy.
#[must_use]
pub(crate) fn base_label_for_project_root(project_root: &Path, strategy: DomainStrategy) -> String {
    match strategy {
        DomainStrategy::Directory => {
            let project_name = project_root
                .file_name()
                .and_then(std::ffi::OsStr::to_str)
                .unwrap_or("my-app");
            sanitize_project_slug(project_name)
        }
        DomainStrategy::Random => random_project_label(project_root),
    }
}

/// Builds a generated primary domain for the given service name.
#[must_use]
pub(crate) fn generated_domain(base_label: &str, service_name: &str) -> String {
    if service_name == "app" {
        return format!("{base_label}.berth");
    }

    format!("{base_label}-{service_name}.berth")
}

fn random_project_label(project_root: &Path) -> String {
    let canonical = project_root
        .canonicalize()
        .unwrap_or_else(|_| project_root.to_path_buf());
    let normalized = canonical.to_string_lossy();
    let hash = fnv1a_64(normalized.as_bytes());
    format!("berth-{:08x}", hash & 0xffff_ffff)
}

fn fnv1a_64(input: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;

    for byte in input {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }

    hash
}

#[cfg(test)]
mod tests {
    use super::{base_label_for_project_root, generated_domain, sanitize_project_slug};
    use crate::config::DomainStrategy;
    use std::path::Path;

    #[test]
    fn sanitize_project_slug_normalizes_mixed_names() {
        assert_eq!(sanitize_project_slug("Billing API"), "billing-api");
        assert_eq!(sanitize_project_slug("___"), "my-app");
    }

    #[test]
    fn generated_domain_uses_plain_base_for_app_service() {
        assert_eq!(generated_domain("my-project", "app"), "my-project.berth");
        assert_eq!(
            generated_domain("my-project", "mailhog"),
            "my-project-mailhog.berth"
        );
    }

    #[test]
    fn random_base_label_is_stable_for_same_root() {
        let root = Path::new("/tmp/example-project");
        let first = base_label_for_project_root(root, DomainStrategy::Random);
        let second = base_label_for_project_root(root, DomainStrategy::Random);

        assert_eq!(first, second);
        assert!(first.starts_with("berth-"));
    }
}
