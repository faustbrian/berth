//! Normalization helper for `php artisan` ANSI flags.

/// Forces `--ansi` for artisan commands and strips `--no-ansi`.
pub(super) fn ensure_artisan_ansi(command: Vec<String>, tty: bool) -> Vec<String> {
    with_artisan_ansi(command, tty)
}

/// Adds `--ansi` to `php artisan` commands only when the command runs with a
/// TTY (berth decides that from its own stdin/stdout and `--tty`/`--no-tty`);
/// an explicit `--ansi` or `--no-ansi` from the caller always wins. Agents
/// read plain output from pipes, so color is never forced on them.
fn with_artisan_ansi(mut command: Vec<String>, tty: bool) -> Vec<String> {
    let is_artisan = command.first().map(String::as_str) == Some("php")
        && command.get(1).map(String::as_str) == Some("artisan");
    let explicit = command
        .iter()
        .any(|arg| arg == "--ansi" || arg == "--no-ansi");
    if is_artisan && tty && !explicit {
        command.push("--ansi".to_owned());
    }
    command
}

#[cfg(test)]
mod tests {
    use super::with_artisan_ansi;

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_owned()).collect()
    }

    #[test]
    fn non_artisan_commands_are_unchanged() {
        assert_eq!(
            with_artisan_ansi(args(&["php", "-v"]), true),
            args(&["php", "-v"])
        );
    }

    #[test]
    fn ansi_is_added_only_for_terminals() {
        let command = args(&["php", "artisan", "about"]);
        assert_eq!(
            with_artisan_ansi(command.clone(), true),
            args(&["php", "artisan", "about", "--ansi"])
        );
        assert_eq!(with_artisan_ansi(command.clone(), false), command);
    }

    #[test]
    fn explicit_color_flags_are_kept() {
        let no_ansi = args(&["php", "artisan", "about", "--no-ansi"]);
        assert_eq!(with_artisan_ansi(no_ansi.clone(), true), no_ansi);
        let ansi = args(&["php", "artisan", "about", "--ansi"]);
        assert_eq!(with_artisan_ansi(ansi.clone(), false), ansi);
    }
}
