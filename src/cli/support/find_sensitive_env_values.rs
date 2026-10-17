//! cli support find sensitive env values module.
//!
//! Contains cli support find sensitive env values logic used by Berth command workflows.

use anyhow::{Context, Result};
use std::path::Path;

pub(crate) fn find_sensitive_env_values(env_path: &Path) -> Result<Vec<String>> {
    let content = std::fs::read_to_string(env_path)
        .with_context(|| format!("failed to read {}", env_path.display()))?;
    let sensitive_prefixes = [
        "APP_KEY",
        "DB_PASSWORD",
        "MAIL_PASSWORD",
        "AWS_SECRET_ACCESS_KEY",
        "MEILISEARCH_KEY",
        "TYPESENSE_API_KEY",
        "REDIS_PASSWORD",
    ];
    let mut matched = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let Some((key, raw_value)) = trimmed.split_once('=') else {
            continue;
        };
        let value = raw_value.trim().trim_matches('"').trim_matches('\'');
        if sensitive_prefixes
            .iter()
            .any(|prefix| key.starts_with(prefix))
            && !value.is_empty()
            && value != "null"
            && !value.eq_ignore_ascii_case("changeme")
            && !value.eq_ignore_ascii_case("secret")
            && !value.eq_ignore_ascii_case("password")
        {
            matched.push(key.to_owned());
        }
    }
    Ok(matched)
}

#[cfg(test)]
mod tests {
    use super::find_sensitive_env_values;

    #[test]
    fn find_sensitive_env_values_detects_known_keys_and_filters_values()
    -> Result<(), std::io::Error> {
        let mut env_path = std::env::temp_dir();
        env_path.push("berth-sensitive-env-test");
        let mut file = std::fs::File::create(&env_path)?;
        use std::io::Write;
        writeln!(
            file,
            "APP_KEY=base64:abc\nDB_PASSWORD=secret123\nMAIL_PASSWORD=\"\" \nCHANGED=foo\nEMPTY_KEY=\n",
        )?;
        drop(file);

        let found = find_sensitive_env_values(&env_path).expect("found env keys");
        assert!(found.contains(&"APP_KEY".to_owned()));
        assert!(found.contains(&"DB_PASSWORD".to_owned()));
        assert!(!found.contains(&"MAIL_PASSWORD".to_owned()));
        assert!(!found.contains(&"CHANGED".to_owned()));
        assert!(!found.contains(&"EMPTY_KEY".to_owned()));

        std::fs::remove_file(env_path)?;
        Ok(())
    }
}
