//! Project-level app domain naming helpers.
//!
//! Contains shared domain name generation logic used by config workflows.

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

#[cfg(test)]
mod tests {
    use super::sanitize_project_slug;

    #[test]
    fn sanitize_project_slug_normalizes_mixed_names() {
        assert_eq!(sanitize_project_slug("Billing API"), "billing-api");
        assert_eq!(sanitize_project_slug("___"), "my-app");
    }
}
