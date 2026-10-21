//! Browser-test runtime helpers for derived app images.

use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;

const BROWSER_TEST_RUNTIME_ENV_KEY: &str = "BERTH_BROWSER_TEST_RUNTIME";
const BROWSER_TEST_RUNTIME_TARGETS_ENV_KEY: &str = "BERTH_BROWSER_TEST_RUNTIME_TARGETS";

/// Marks a runtime env map so derived app images include browser-test deps.
pub(crate) fn enable_browser_test_runtime(
    app_env: &mut HashMap<String, String>,
    target_names: &[String],
) {
    app_env.insert(BROWSER_TEST_RUNTIME_ENV_KEY.to_owned(), "1".to_owned());
    app_env.insert(
        BROWSER_TEST_RUNTIME_TARGETS_ENV_KEY.to_owned(),
        target_names.join(","),
    );
}

/// Returns whether the injected env enables browser-test runtime behavior.
pub(super) fn browser_test_runtime_enabled(
    app_env: &HashMap<String, String>,
    target_name: &str,
) -> bool {
    app_env
        .get(BROWSER_TEST_RUNTIME_ENV_KEY)
        .is_some_and(|value| value == "1")
        && app_env
            .get(BROWSER_TEST_RUNTIME_TARGETS_ENV_KEY)
            .is_some_and(|targets| {
                targets
                    .split(',')
                    .map(str::trim)
                    .any(|candidate| candidate == target_name)
            })
}

/// Resolves the Playwright package spec used for build-time dependency install.
pub(super) fn resolve_playwright_package_spec(workspace_root: &Path) -> Option<String> {
    let package_json = std::fs::read_to_string(workspace_root.join("package.json")).ok()?;
    let parsed: Value = serde_json::from_str(&package_json).ok()?;

    dependency_version(&parsed, "devDependencies", "playwright")
        .or_else(|| dependency_version(&parsed, "dependencies", "playwright"))
        .or_else(|| dependency_version(&parsed, "devDependencies", "@playwright/test"))
        .or_else(|| dependency_version(&parsed, "dependencies", "@playwright/test"))
        .map(|version| format!("playwright@{version}"))
        .or_else(|| Some("playwright".to_owned()))
}

fn dependency_version<'a>(root: &'a Value, section: &str, package: &str) -> Option<&'a str> {
    root.get(section)
        .and_then(Value::as_object)
        .and_then(|deps| deps.get(package))
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
}

#[cfg(test)]
mod tests {
    use super::{
        browser_test_runtime_enabled, enable_browser_test_runtime, resolve_playwright_package_spec,
    };
    use std::collections::HashMap;
    use std::path::PathBuf;

    fn temp_root(name: &str) -> PathBuf {
        let root = crate::test_support::temp_root().join(format!(
            "berth-browser-runtime-{name}-{}",
            crate::test_support::unique_stamp()
        ));
        std::fs::create_dir_all(&root).expect("create temp root");
        root
    }

    #[test]
    fn enable_browser_test_runtime_marks_runtime_env() {
        let mut env = HashMap::new();
        let targets = vec!["app".to_owned()];

        enable_browser_test_runtime(&mut env, &targets);

        assert!(browser_test_runtime_enabled(&env, "app"));
        assert!(!browser_test_runtime_enabled(&env, "mailhog"));
    }

    #[test]
    fn resolves_playwright_package_spec_from_playwright_dependency() {
        let root = temp_root("playwright");
        std::fs::write(
            root.join("package.json"),
            r#"{
                "devDependencies": {
                    "playwright": "^1.60.0"
                }
            }"#,
        )
        .expect("write package.json");

        assert_eq!(
            resolve_playwright_package_spec(&root),
            Some("playwright@^1.60.0".to_owned())
        );
    }

    #[test]
    fn resolves_playwright_package_spec_from_playwright_test_dependency() {
        let root = temp_root("playwright-test");
        std::fs::write(
            root.join("package.json"),
            r#"{
                "devDependencies": {
                    "@playwright/test": "1.55.0"
                }
            }"#,
        )
        .expect("write package.json");

        assert_eq!(
            resolve_playwright_package_spec(&root),
            Some("playwright@1.55.0".to_owned())
        );
    }
}
