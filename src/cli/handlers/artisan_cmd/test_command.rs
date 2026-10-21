//! cli handlers artisan cmd test command module.
//!
//! Contains cli handlers artisan cmd test command logic used by Berth command workflows.

use std::path::Path;

use serde_json::Value;
use std::collections::HashMap;

/// Builds artisan test command for command execution.
pub(super) fn build_artisan_test_command(
    user_command: Vec<String>,
    app_env: &HashMap<String, String>,
    bootstrap_playwright: bool,
) -> Vec<String> {
    let mut artisan_parts = vec!["php".to_owned(), "artisan".to_owned()];
    artisan_parts.extend(user_command);
    let escaped = artisan_parts
        .iter()
        .map(|part| shell_single_quote(part))
        .collect::<Vec<_>>()
        .join(" ");

    let mut exports: Vec<String> = app_env
        .iter()
        .filter(|&(key, _value)| is_valid_env_key(key))
        .map(|(key, value)| format!("export {key}={}", shell_single_quote(value)))
        .collect();
    exports.sort();
    exports.push("export APP_ENV='testing'".to_owned());

    let playwright_bootstrap = if bootstrap_playwright {
        " && if [ ! -f package.json ]; then echo 'Playwright browser tests require package.json in the workspace root.' >&2; exit 1; fi && export PLAYWRIGHT_BROWSERS_PATH='/app/.berth/cache/playwright' && mkdir -p \"$PLAYWRIGHT_BROWSERS_PATH\" && if [ ! -x ./node_modules/.bin/playwright ]; then npm install; fi && npx playwright install chromium"
    } else {
        ""
    };

    let script = format!(
        "mkdir -p /tmp/berth-php/conf.d \\\n&& printf 'memory_limit=4096M\\n' > /tmp/berth-php/conf.d/zz-berth-memory.ini \\\n&& export PHP_INI_SCAN_DIR='/tmp/berth-php/conf.d:/usr/local/etc/php/conf.d' \\\n&& {}{playwright_bootstrap} \\\n&& {escaped}",
        exports.join(" && ")
    );
    vec!["sh".to_owned(), "-lc".to_owned(), script]
}

pub(super) fn should_bootstrap_playwright(workspace_root: &Path) -> bool {
    let composer_json_path = workspace_root.join("composer.json");
    let composer_json = match std::fs::read_to_string(composer_json_path) {
        Ok(content) => content,
        Err(_) => return false,
    };

    let parsed: Value = match serde_json::from_str(&composer_json) {
        Ok(value) => value,
        Err(_) => return false,
    };

    has_dependency(&parsed, "require", "pestphp/pest-plugin-browser")
        || has_dependency(&parsed, "require-dev", "pestphp/pest-plugin-browser")
}

fn has_dependency(root: &Value, section: &str, dependency: &str) -> bool {
    root.get(section)
        .and_then(Value::as_object)
        .is_some_and(|deps| deps.contains_key(dependency))
}

fn shell_single_quote(value: &str) -> String {
    let escaped = value.replace('\'', "'\"'\"'");
    format!("'{escaped}'")
}

/// Returns whether the key matches the accepted environment-variable format.
fn is_valid_env_key(key: &str) -> bool {
    let mut chars = key.chars();
    match chars.next() {
        Some(ch) if ch.is_ascii_alphabetic() || ch == '_' => {}
        _ => return false,
    }
    chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

#[cfg(test)]
mod tests {
    use super::{build_artisan_test_command, should_bootstrap_playwright};
    use std::collections::HashMap;
    use std::path::PathBuf;

    fn temp_root(name: &str) -> PathBuf {
        let root = crate::test_support::temp_root().join(format!(
            "berth-artisan-test-command-{name}-{}",
            crate::test_support::unique_stamp()
        ));
        std::fs::create_dir_all(&root).expect("create temp root");
        root
    }

    #[test]
    fn should_bootstrap_playwright_when_browser_plugin_present() {
        let root = temp_root("playwright-enabled");
        let composer = root.join("composer.json");
        std::fs::write(
            composer,
            r#"{
                "require-dev": {
                    "pestphp/pest-plugin-browser": "^4.3"
                }
            }"#,
        )
        .expect("write composer");

        assert!(should_bootstrap_playwright(&root));
    }

    #[test]
    fn should_not_bootstrap_playwright_without_browser_plugin() {
        let root = temp_root("playwright-disabled");
        let composer = root.join("composer.json");
        std::fs::write(
            composer,
            r#"{
                "require-dev": {
                    "pestphp/pest": "^4.0"
                }
            }"#,
        )
        .expect("write composer");

        assert!(!should_bootstrap_playwright(&root));
    }

    #[test]
    fn artisan_test_command_includes_playwright_bootstrap_when_enabled() {
        let command = build_artisan_test_command(vec!["test".to_owned()], &HashMap::new(), true);
        let script = command.get(2).expect("shell script payload");
        assert!(script.contains("if [ ! -f package.json ]"));
        assert!(script.contains("export PLAYWRIGHT_BROWSERS_PATH="));
        assert!(script.contains("mkdir -p \"$PLAYWRIGHT_BROWSERS_PATH\""));
        assert!(
            script.contains("if [ ! -x ./node_modules/.bin/playwright ]; then npm install; fi")
        );
        assert!(script.contains("npx playwright install chromium"));
        assert!(!script.contains("npx playwright install-deps"));
        assert!(!script.contains("npx playwright install &&"));
    }
}
