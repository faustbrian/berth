//! cli support normalize path module.
//!
//! Contains cli support normalize path logic used by Berth command workflows.

/// Normalizes path into a canonical form.
pub(crate) fn normalize_path(path: &str) -> String {
    if path.starts_with('/') {
        path.to_owned()
    } else {
        format!("/{path}")
    }
}

#[cfg(test)]
mod tests {
    use super::normalize_path;

    #[test]
    fn normalize_path_preserves_absolute_path() {
        assert_eq!(normalize_path("/tmp/berth"), "/tmp/berth");
    }

    #[test]
    fn normalize_path_prefixes_relative_path() {
        assert_eq!(normalize_path("config/app.env"), "/config/app.env");
    }
}
