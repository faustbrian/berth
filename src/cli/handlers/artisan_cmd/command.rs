//! cli handlers artisan cmd command module.
//!
//! Contains cli handlers artisan cmd command logic used by Berth command workflows.

use crate::cli;

/// Ensures an artisan subcommand exists by defaulting to `list`.
pub(super) fn ensure_artisan_subcommand(command: Vec<String>) -> Vec<String> {
    if command.is_empty() {
        return vec!["list".to_owned()];
    }
    command
}

/// Returns whether the artisan command is a `test` invocation.
pub(super) fn is_artisan_test_command(command: &[String]) -> bool {
    command
        .first()
        .is_some_and(|subcommand| subcommand.eq_ignore_ascii_case("test"))
}

/// Removes artisan env overrides as part of the cli handlers artisan cmd command workflow.
pub(super) fn remove_artisan_env_overrides(command: &[String]) -> Vec<String> {
    let mut cleaned = Vec::with_capacity(command.len());
    let mut skip_next = false;
    for arg in command {
        if skip_next {
            skip_next = false;
            continue;
        }
        if arg == "--env" {
            skip_next = true;
            continue;
        }
        if arg.starts_with("--env=") {
            continue;
        }
        cleaned.push(arg.clone());
    }
    cleaned
}

/// Builds artisan command for command execution.
pub(super) fn build_artisan_command(user_command: Vec<String>) -> Vec<String> {
    cli::support::build_artisan_command(user_command)
}

/// Ensures artisan ansi flag exists and is in the required state.
pub(super) fn ensure_artisan_ansi_flag(command: Vec<String>) -> Vec<String> {
    let mut normalized: Vec<String> = command
        .into_iter()
        .filter(|arg| arg != "--no-ansi")
        .collect();

    if !normalized.iter().any(|arg| arg == "--ansi") {
        normalized.push("--ansi".to_owned());
    }

    normalized
}

/// Resolves artisan tty using configured inputs and runtime state.
pub(super) fn resolve_artisan_tty(tty: bool, no_tty: bool) -> bool {
    cli::support::effective_tty(tty, no_tty)
}

#[cfg(test)]
mod tests {
    use super::{
        ensure_artisan_ansi_flag, ensure_artisan_subcommand, is_artisan_test_command,
        remove_artisan_env_overrides,
    };
    use crate::cli::support::effective_tty;

    #[test]
    fn detects_artisan_test_command_case_insensitive() {
        assert!(is_artisan_test_command(&["TEST".to_owned()]));
        assert!(!is_artisan_test_command(&["migrate".to_owned()]));
        assert!(!is_artisan_test_command(&[]));
    }

    #[test]
    fn ensure_artisan_subcommand_defaults_to_list() {
        assert_eq!(
            ensure_artisan_subcommand(Vec::new()),
            vec!["list".to_owned()]
        );
        assert_eq!(
            ensure_artisan_subcommand(vec!["about".to_owned()]),
            vec!["about".to_owned()]
        );
    }

    #[test]
    fn remove_artisan_env_overrides_strips_flag_forms() {
        let command = vec![
            "config:clear".to_owned(),
            "--env".to_owned(),
            "local".to_owned(),
            "--verbose".to_owned(),
            "--env=production".to_owned(),
            "test".to_owned(),
        ];

        assert_eq!(
            remove_artisan_env_overrides(&command),
            vec![
                "config:clear".to_owned(),
                "--verbose".to_owned(),
                "test".to_owned(),
            ]
        );
    }

    #[test]
    fn ensure_artisan_ansi_flag_removes_no_ansi_and_adds_ansi() {
        let command = vec![
            "tinker".to_owned(),
            "--no-ansi".to_owned(),
            "--help".to_owned(),
        ];

        assert_eq!(
            ensure_artisan_ansi_flag(command),
            vec![
                "tinker".to_owned(),
                "--help".to_owned(),
                "--ansi".to_owned(),
            ]
        );
    }

    #[test]
    fn resolve_artisan_tty_respects_explicit_flags() {
        assert!(!super::resolve_artisan_tty(false, true));
        assert_eq!(
            super::resolve_artisan_tty(true, false),
            effective_tty(false, false)
        );
    }

    #[test]
    fn resolve_artisan_tty_uses_effective_tty_default_path() {
        assert!(!super::resolve_artisan_tty(false, true));
        assert_eq!(
            effective_tty(true, false),
            super::resolve_artisan_tty(true, false)
        );
        assert_eq!(
            effective_tty(false, false),
            super::resolve_artisan_tty(false, false)
        );
    }
}
