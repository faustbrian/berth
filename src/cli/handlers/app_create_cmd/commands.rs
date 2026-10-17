//! cli handlers app create cmd commands module.
//!
//! Contains cli handlers app create cmd commands logic used by Berth command workflows.

pub(super) fn setup_commands() -> Vec<Vec<String>> {
    vec![
        vec![
            "php".to_owned(),
            "artisan".to_owned(),
            "key:generate".to_owned(),
            "--ansi".to_owned(),
            "--force".to_owned(),
        ],
        vec![
            "php".to_owned(),
            "artisan".to_owned(),
            "optimize:clear".to_owned(),
            "--ansi".to_owned(),
        ],
    ]
}

pub(super) fn storage_link_command() -> Vec<String> {
    vec![
        "php".to_owned(),
        "artisan".to_owned(),
        "storage:link".to_owned(),
        "--ansi".to_owned(),
        "--force".to_owned(),
    ]
}

pub(super) fn migrate_command() -> Vec<String> {
    vec![
        "php".to_owned(),
        "artisan".to_owned(),
        "migrate".to_owned(),
        "--isolated".to_owned(),
        "--ansi".to_owned(),
        "--force".to_owned(),
    ]
}

pub(super) fn seed_command() -> Vec<String> {
    vec![
        "php".to_owned(),
        "artisan".to_owned(),
        "db:seed".to_owned(),
        "--ansi".to_owned(),
        "--force".to_owned(),
    ]
}

#[cfg(test)]
mod tests {
    use super::{migrate_command, seed_command, setup_commands, storage_link_command};

    #[test]
    fn setup_commands_are_expected() {
        let commands = setup_commands();
        assert_eq!(commands.len(), 2);
        assert_eq!(commands[0][0], "php");
        assert_eq!(commands[1][0], "php");
    }

    #[test]
    fn storage_link_command_uses_expected_shape() {
        let command = storage_link_command();
        assert_eq!(command.len(), 5);
        assert_eq!(command[0], "php");
        assert_eq!(command[1], "artisan");
        assert_eq!(command[2], "storage:link");
    }

    #[test]
    fn migrate_and_seed_command_defaults_present() {
        let migrate = migrate_command();
        let seed = seed_command();
        assert!(migrate.contains(&"--force".to_string()));
        assert!(seed.contains(&"--force".to_string()));
    }
}
