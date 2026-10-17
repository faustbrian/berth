//! MySQL/MariaDB schema-dump compatibility shims for serve exec flows.

use anyhow::Result;
use std::collections::HashMap;

use crate::config::ServiceConfig;
use crate::serve::sql_client_flavor::sql_client_flavor_from_injected_env;

use super::container::exec_setup_as_root;
mod scripts;
use scripts::{install_mysql_wrapper_script, install_wrapper_script};

/// Installs the `mysql` wrapper into a running serve container.
pub(super) fn ensure_mysql_cli_compat(target: &ServiceConfig) -> Result<()> {
    let command = vec![
        "sh".to_owned(),
        "-lc".to_owned(),
        install_mysql_wrapper_script().to_owned(),
    ];
    exec_setup_as_root(target, &command)
}

/// Installs `mysqldump` wrapper when about to run `artisan schema:dump`.
pub(super) fn ensure_schema_dump_compat(
    target: &ServiceConfig,
    artisan_args: &[String],
) -> Result<()> {
    if !is_schema_dump_command(artisan_args) {
        return Ok(());
    }

    let command = vec![
        "sh".to_owned(),
        "-lc".to_owned(),
        install_wrapper_script("mysql"),
    ];

    // No TTY required; install script into the running app container
    // before running `php artisan schema:dump`.
    exec_setup_as_root(target, &command)
}

/// Installs `mysqldump` wrapper for direct command execution paths.
pub(super) fn ensure_schema_dump_compat_for_command(
    target: &ServiceConfig,
    command: &[String],
    injected_env: &HashMap<String, String>,
) -> Result<()> {
    if !is_schema_dump_exec_command(command) {
        return Ok(());
    }

    let sql_client_flavor = sql_client_flavor_from_injected_env(injected_env);

    let install = vec![
        "sh".to_owned(),
        "-lc".to_owned(),
        install_wrapper_script(sql_client_flavor.as_str()),
    ];

    exec_setup_as_root(target, &install)
}

/// Returns whether the artisan subcommand is `schema:dump`.
fn is_schema_dump_command(artisan_args: &[String]) -> bool {
    artisan_args
        .first()
        .is_some_and(|arg| arg.eq_ignore_ascii_case("schema:dump"))
}

/// Returns whether a full command vector represents `php artisan schema:dump`.
fn is_schema_dump_exec_command(command: &[String]) -> bool {
    command.first().map(String::as_str) == Some("php")
        && command.get(1).map(String::as_str) == Some("artisan")
        && command
            .get(2)
            .is_some_and(|arg| arg.eq_ignore_ascii_case("schema:dump"))
}

#[cfg(test)]
mod tests {
    use super::{is_schema_dump_command, is_schema_dump_exec_command, scripts};

    #[test]
    fn detects_schema_dump_command() {
        assert!(is_schema_dump_command(&["schema:dump".to_owned()]));
    }

    #[test]
    fn ignores_other_artisan_commands() {
        assert!(!is_schema_dump_command(&["migrate".to_owned()]));
    }

    #[test]
    fn ignores_empty_commands() {
        assert!(!is_schema_dump_command(&[]));
    }

    #[test]
    fn detects_schema_dump_exec_command() {
        assert!(is_schema_dump_exec_command(&[
            "php".to_owned(),
            "artisan".to_owned(),
            "schema:dump".to_owned(),
            "--ansi".to_owned(),
        ]));
    }

    #[test]
    fn ignores_non_schema_exec_command() {
        assert!(!is_schema_dump_exec_command(&[
            "php".to_owned(),
            "artisan".to_owned(),
            "migrate".to_owned(),
        ]));
    }

    #[test]
    fn mysql_wrapper_script_disables_ssl_on_self_signed_errors() {
        let script = scripts::install_mysql_wrapper_script();
        assert!(script.contains("/usr/local/bin/mysql"));
        assert!(script.contains("--ssl-mode=DISABLED"));
        assert!(script.contains("--skip-ssl"));
    }
}
