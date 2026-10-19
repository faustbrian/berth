//! The error for a swarm target whose checkout has no `.berth.yaml`.

use std::path::{Path, PathBuf};

/// The `.berth.yaml` of swarm target `name` at `root`. A checkout without
/// one, including one that still has a helm or stackctl file, is someone
/// else's working copy on an older branch: it gets [`missing_target_config`]
/// instead of the advice to migrate it in place.
///
/// # Errors
///
/// Returns an error when the checkout has no `.berth.yaml`.
pub(crate) fn target_config_path(name: &str, root: &Path) -> anyhow::Result<PathBuf> {
    let path = root.join(crate::config::project_file::PROJECT_FILE_NAME);
    if !path.is_file() {
        return Err(missing_target_config(name, root));
    }
    Ok(path)
}

/// Explains why swarm target `name` at `root` cannot be used: berth clones
/// only a missing checkout and never switches or pulls an existing one, so
/// a checkout on a branch without `.berth.yaml` (one that predates berth,
/// say) has to be updated by its owner.
fn missing_target_config(name: &str, root: &Path) -> anyhow::Error {
    let at = root.display();
    if !root.is_dir() {
        // Only `up` and `swarm up` bootstrap targets; `artisan test`,
        // `up --no-deps` and the other commands read them where they are.
        return anyhow::anyhow!(
            "swarm target '{name}' has no checkout at {at}; clone it there, or run `berth up` \
             or `berth swarm up`, which clone a missing target that has `git`"
        );
    }
    let fix = "check out a branch that has one (usually its default branch); berth clones only \
               a missing checkout and never switches or pulls an existing one";
    let legacy: Vec<&str> = crate::config::project_file::LEGACY_CONFIG_FILE_NAMES
        .into_iter()
        .filter(|file| root.join(file).is_file())
        .collect();
    if legacy.is_empty() {
        anyhow::anyhow!("swarm target '{name}' at {at} has no .berth.yaml: {fix}")
    } else {
        anyhow::anyhow!(
            "swarm target '{name}' at {at} has no .berth.yaml, only {}: the checkout predates \
             berth, so {fix}",
            legacy.join(", ")
        )
    }
}

#[cfg(test)]
mod tests {
    use super::missing_target_config;

    fn root(label: &str) -> std::path::PathBuf {
        let root = crate::test_support::temp_root().join(format!(
            "berth-missing-target-{label}-{}",
            std::process::id()
        ));
        drop(std::fs::remove_dir_all(&root));
        root
    }

    #[test]
    fn a_checkout_that_predates_berth_is_named_with_the_way_out() {
        let root = root("legacy");
        std::fs::create_dir_all(&root).expect("root");
        std::fs::write(root.join(".helm.toml"), "").expect("legacy");
        let message = format!("{:#}", missing_target_config("bill", &root));
        assert!(message.contains("swarm target 'bill'"), "{message}");
        assert!(message.contains(".helm.toml"), "{message}");
        assert!(message.contains("predates berth"), "{message}");
        assert!(message.contains("default branch"), "{message}");
        assert!(message.contains("never switches or pulls"), "{message}");
        assert!(!message.contains("config migrate"), "{message}");
        std::fs::remove_dir_all(&root).expect("cleanup");
    }

    #[test]
    fn a_checkout_without_any_config_is_told_to_switch_branches() {
        let root = root("plain");
        std::fs::create_dir_all(&root).expect("root");
        let message = format!("{:#}", missing_target_config("bill", &root));
        assert!(message.contains("has no .berth.yaml"), "{message}");
        assert!(message.contains("default branch"), "{message}");
        assert!(!message.contains("predates berth"), "{message}");
    }

    #[test]
    fn a_missing_checkout_says_it_is_missing() {
        let root = root("absent");
        let message = format!("{:#}", missing_target_config("bill", &root));
        assert!(message.contains("no checkout"), "{message}");
        assert!(message.contains("`berth swarm up`"), "{message}");
        assert!(!message.contains("give the target"), "{message}");
    }
}
