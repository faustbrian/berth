//! cli support scrub env file module.
//!
//! Contains cli support scrub env file logic used by Berth command workflows.

use anyhow::{Context, Result};
use std::path::Path;

pub(crate) fn scrub_env_file(env_path: &Path) -> Result<usize> {
    let content = std::fs::read_to_string(env_path)
        .with_context(|| format!("failed to read {}", env_path.display()))?;
    let mut updated = 0_usize;
    let scrubbed = content
        .lines()
        .map(|line| {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                return line.to_owned();
            }
            let Some((key, _)) = line.split_once('=') else {
                return line.to_owned();
            };
            let replacement = match key.trim() {
                "APP_KEY" => Some("APP_KEY=\"\""),
                "DB_PASSWORD" => Some("DB_PASSWORD=\"local\""),
                "MAIL_PASSWORD" => Some("MAIL_PASSWORD=\"\""),
                "AWS_SECRET_ACCESS_KEY" => Some("AWS_SECRET_ACCESS_KEY=\"miniosecret\""),
                "MEILISEARCH_KEY" => Some("MEILISEARCH_KEY=\"masterKey\""),
                "TYPESENSE_API_KEY" => Some("TYPESENSE_API_KEY=\"xyz\""),
                "REDIS_PASSWORD" => Some("REDIS_PASSWORD=\"\""),
                _ => None,
            };
            if let Some(new_line) = replacement {
                updated += 1;
                return new_line.to_owned();
            }
            line.to_owned()
        })
        .collect::<Vec<_>>()
        .join("\n");

    std::fs::write(env_path, format!("{scrubbed}\n"))
        .with_context(|| format!("failed to write {}", env_path.display()))?;
    Ok(updated)
}

#[cfg(test)]
mod tests {
    use super::scrub_env_file;

    use std::fs;
    use std::path::PathBuf;

    fn temp_env_file(contents: &str) -> PathBuf {
        let path = crate::test_support::temp_root().join(format!(
            "berth-scrub-env-{}-{}.env",
            std::process::id(),
            contents.len()
        ));
        fs::write(&path, contents).expect("write fixture");
        path
    }

    #[test]
    fn scrub_env_file_replaces_known_keys_and_ignores_comments() {
        let env_path = temp_env_file(
            "# preexisting\n\
             APP_KEY=old\n\
             DB_PASSWORD=old\n\
             MAIL_PASSWORD=old\n\
             AWS_SECRET_ACCESS_KEY=old\n\
             TYPESENSE_API_KEY=old\n\
             MEILISEARCH_KEY=old\n\
             REDIS_PASSWORD=old\n\
             IGNORE_ME=keep\n",
        );
        let updated = scrub_env_file(&env_path).expect("scrub env");
        assert_eq!(updated, 7);

        let content = fs::read_to_string(&env_path).expect("read scrubbed env");
        assert!(content.contains("APP_KEY=\"\""));
        assert!(content.contains("DB_PASSWORD=\"local\""));
        assert!(content.contains("MAIL_PASSWORD=\"\""));
        assert!(content.contains("AWS_SECRET_ACCESS_KEY=\"miniosecret\""));
        assert!(content.contains("TYPESENSE_API_KEY=\"xyz\""));
        assert!(content.contains("MEILISEARCH_KEY=\"masterKey\""));
        assert!(content.contains("REDIS_PASSWORD=\"\""));
        assert!(content.contains("IGNORE_ME=keep"));
    }

    #[test]
    fn scrub_env_file_leaves_blank_and_non_matching_lines() {
        let env_path = temp_env_file(" \n# comment\nINVALID\nPORT=3306\n");
        let updated = scrub_env_file(&env_path).expect("scrub env");
        assert_eq!(updated, 0);

        let content = fs::read_to_string(&env_path).expect("read scrubbed env");
        assert!(content.contains("INVALID"));
        assert!(content.ends_with('\n'));
    }
}
