//! The error for a swarm target whose checkout has no `.berth.yaml`.

use std::path::{Path, PathBuf};

use crate::config::SwarmGit;

/// The `.berth.yaml` of swarm target `name` at `root`. A checkout without
/// one, including one that still has a helm or stackctl file, is someone
/// else's working copy on an older branch: it gets [`missing_target_config`]
/// instead of the advice to migrate it in place. `git` is the target's
/// configured clone source, which decides the advice.
///
/// # Errors
///
/// Returns an error when the checkout has no `.berth.yaml`.
pub(crate) fn target_config_path(
    name: &str,
    root: &Path,
    git: Option<&SwarmGit>,
) -> anyhow::Result<PathBuf> {
    let path = root.join(crate::config::project_file::PROJECT_FILE_NAME);
    if !path.is_file() {
        return Err(missing_target_config(name, root, git));
    }
    Ok(path)
}

/// Explains why swarm target `name` at `root` cannot be used: berth clones
/// only a missing checkout and never switches or pulls an existing one, so
/// a checkout on a branch without `.berth.yaml` (one that predates berth,
/// say) has to be updated by its owner, or by the workspace's `git.branch`
/// when that pin is the branch without one.
fn missing_target_config(name: &str, root: &Path, git: Option<&SwarmGit>) -> anyhow::Error {
    let at = root.display();
    let dir = crate::followup::quote(&root.display().to_string());
    if root.exists() && !root.is_dir() {
        // Bootstrap skips any existing path, so `up` would never clone here.
        return anyhow::anyhow!(
            "swarm target '{name}' root {at} is a file, not a directory; move or remove that \
             file, or point the target's `root` in the workspace `.berth.yaml` at its checkout"
        );
    }
    if !root.is_dir() {
        // Only `up` and `swarm up` bootstrap targets, and only those with
        // `git`; `artisan test`, `up --no-deps` and the other commands read
        // them where they are.
        return match git {
            Some(git) => anyhow::anyhow!(
                "swarm target '{name}' has no checkout at {at}; run {} (or `berth swarm up` \
                 in the workspace) to clone it from {}, or clone it there yourself",
                crate::followup::command("up"),
                git.repo
            ),
            None => anyhow::anyhow!(
                "swarm target '{name}' has no checkout at {at} and no `git` repo to clone it \
                 from; clone it there, or add `git` to the target in the workspace `.berth.yaml`"
            ),
        };
    }
    let legacy: Vec<&str> = crate::config::project_file::LEGACY_CONFIG_FILE_NAMES
        .into_iter()
        .filter(|file| root.join(file).is_file())
        .collect();
    let what = if legacy.is_empty() {
        format!("swarm target '{name}' at {at} has no .berth.yaml")
    } else {
        format!(
            "swarm target '{name}' at {at} has no .berth.yaml, only {}: the checkout predates berth",
            legacy.join(", ")
        )
    };
    match git.and_then(|git| git.branch.as_deref()) {
        // berth cannot tell which branch an existing checkout is on, so the
        // pin is only a suspect.
        Some(branch) => anyhow::anyhow!(
            "{what}; check out the branch the workspace pins, '{branch}' (`git.branch`). A \
             checkout berth cloned tracks only one branch, so run `git -C {dir} remote \
             set-branches --add origin {pin}`, `git -C {dir} fetch origin` and `git -C {dir} \
             checkout {pin}` (if it is a clone berth made and holds none of your work, \
             deleting it so `berth up` clones it again also works); if that branch has no \
             .berth.yaml either, point `git.branch` at one that does (berth never \
             switches or pulls an existing checkout)",
            pin = crate::followup::quote(branch)
        ),
        None => anyhow::anyhow!(
            "{what}; check out a branch that has one (usually its default branch); berth clones \
             only a missing checkout and never switches or pulls an existing one"
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::missing_target_config;
    use crate::config::SwarmGit;

    fn root(label: &str) -> std::path::PathBuf {
        let root = crate::test_support::temp_root().join(format!(
            "berth-missing-target-{label}-{}",
            std::process::id()
        ));
        drop(std::fs::remove_dir_all(&root));
        root
    }

    fn git(branch: Option<&str>) -> SwarmGit {
        SwarmGit {
            repo: "git@github.com:acme/bill.git".to_owned(),
            branch: branch.map(ToOwned::to_owned),
        }
    }

    #[test]
    fn a_checkout_that_predates_berth_is_named_with_the_way_out() {
        let root = root("legacy");
        std::fs::create_dir_all(&root).expect("root");
        std::fs::write(root.join(".helm.toml"), "").expect("legacy");
        let message = format!("{:#}", missing_target_config("bill", &root, None));
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
        let message = format!(
            "{:#}",
            missing_target_config("bill", &root, Some(&git(None)))
        );
        assert!(message.contains("has no .berth.yaml"), "{message}");
        assert!(message.contains("default branch"), "{message}");
        assert!(!message.contains("predates berth"), "{message}");
        assert!(!message.contains("git.branch"), "{message}");
        std::fs::remove_dir_all(&root).expect("cleanup");
    }

    #[test]
    fn a_pinned_branch_without_config_names_the_pin_as_the_fix() {
        let root = root("pinned");
        std::fs::create_dir_all(&root).expect("root");
        let message = format!(
            "{:#}",
            missing_target_config("bill", &root, Some(&git(Some("legacy"))))
        );
        assert!(
            message.contains("check out the branch the workspace pins, 'legacy'"),
            "{message}"
        );
        assert!(
            message.contains("if that branch has no .berth.yaml either"),
            "{message}"
        );
        assert!(!message.contains("default branch"), "{message}");
        std::fs::remove_dir_all(&root).expect("cleanup");
    }

    #[test]
    fn a_checkout_switched_off_its_pinned_branch_is_not_blamed_on_the_pin() {
        let root = root("switched");
        std::fs::create_dir_all(&root).expect("root");
        std::fs::write(root.join(".helm.toml"), "").expect("legacy");
        let message = format!(
            "{:#}",
            missing_target_config("bill", &root, Some(&git(Some("develop"))))
        );
        assert!(message.contains("predates berth"), "{message}");
        assert!(
            message.contains("check out the branch the workspace pins, 'develop'"),
            "{message}"
        );
        assert!(!message.contains("which has none"), "{message}");
        std::fs::remove_dir_all(&root).expect("cleanup");
    }

    #[test]
    fn a_pinned_branch_hint_covers_a_single_branch_clone() {
        let root = root("pinned-clone");
        std::fs::create_dir_all(&root).expect("root");
        let message = format!(
            "{:#}",
            missing_target_config("bill", &root, Some(&git(Some("develop"))))
        );
        assert!(message.contains("git -C"), "{message}");
        assert!(
            message.contains("remote set-branches --add origin develop"),
            "{message}"
        );
        assert!(message.contains("holds none of your work"), "{message}");
        std::fs::remove_dir_all(&root).expect("cleanup");
    }

    #[test]
    fn a_root_that_is_a_file_is_named_as_such() {
        let root = root("file");
        std::fs::create_dir_all(root.parent().expect("parent")).expect("parent");
        std::fs::write(&root, "").expect("file");
        let message = format!(
            "{:#}",
            missing_target_config("bill", &root, Some(&git(None)))
        );
        assert!(message.contains("is a file, not a directory"), "{message}");
        assert!(!message.contains("swarm up"), "{message}");
        std::fs::remove_file(&root).expect("cleanup");
    }

    #[test]
    fn a_missing_checkout_with_git_names_the_commands_that_clone_it() {
        let root = root("absent-git");
        let message = format!(
            "{:#}",
            missing_target_config("bill", &root, Some(&git(None)))
        );
        assert!(message.contains("no checkout"), "{message}");
        assert!(message.contains("`berth swarm up`"), "{message}");
        assert!(
            message.contains("git@github.com:acme/bill.git"),
            "{message}"
        );
    }

    #[test]
    fn a_missing_checkout_without_git_is_never_sent_back_to_swarm_up() {
        let root = root("absent");
        let message = format!("{:#}", missing_target_config("bill", &root, None));
        assert!(message.contains("no checkout"), "{message}");
        assert!(message.contains("no `git`"), "{message}");
        assert!(!message.contains("swarm up"), "{message}");
    }
}
