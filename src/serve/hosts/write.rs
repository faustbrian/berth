//! Privileged hosts-file write helpers.

use anyhow::{Context, Result};
use std::io::{self, IsTerminal};
use std::process::Command;

use super::HOSTS_FILE;

/// Appends a hosts entry using `sudo` when direct file append is unavailable.
pub(super) fn append_hosts_entry_with_sudo(domain: &str) -> Result<()> {
    append_hosts_entry_with_runner(
        domain,
        io::stdin().is_terminal(),
        cfg!(target_os = "linux"),
        &mut run_privileged_shell,
    )
}

/// Validates hostnames accepted for hosts-file entry generation.
fn validate_hostname(hostname: &str) -> Result<()> {
    let valid = !hostname.is_empty()
        && hostname
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '.' || ch == '-');
    if valid {
        return Ok(());
    }
    anyhow::bail!("invalid hostname '{hostname}'");
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PrivilegeMethod {
    SudoNonInteractive,
    SudoInteractive,
    Pkexec,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CommandOutcome {
    Success,
    Failure,
    NotFound,
}

fn append_hosts_entry_with_runner<F>(
    domain: &str,
    has_tty: bool,
    is_linux: bool,
    runner: &mut F,
) -> Result<()>
where
    F: FnMut(PrivilegeMethod, &str) -> io::Result<CommandOutcome>,
{
    validate_hostname(domain)?;
    let command = format!("echo '127.0.0.1 {domain}' >> {HOSTS_FILE}");
    let methods = escalation_methods(has_tty, is_linux);

    for method in methods.iter().copied() {
        match runner(method, &command)
            .with_context(|| format!("failed to run {} for hosts entry update", method.command()))?
        {
            CommandOutcome::Success => return Ok(()),
            CommandOutcome::Failure | CommandOutcome::NotFound => continue,
        }
    }

    let interactive_hint = if has_tty {
        String::new()
    } else {
        "\nno interactive terminal detected; rerun Berth from a terminal to allow sudo password prompting"
            .to_owned()
    };

    anyhow::bail!(
        "could not update {HOSTS_FILE} for domain '{domain}' using {}.\n\
         run manually:\n\
           echo '127.0.0.1 {domain}' | sudo tee -a {HOSTS_FILE}{interactive_hint}",
        format_methods(&methods)
    );
}

fn escalation_methods(has_tty: bool, is_linux: bool) -> Vec<PrivilegeMethod> {
    let mut methods = vec![PrivilegeMethod::SudoNonInteractive];
    if has_tty {
        methods.push(PrivilegeMethod::SudoInteractive);
    }
    if is_linux {
        methods.push(PrivilegeMethod::Pkexec);
    }
    methods
}

fn format_methods(methods: &[PrivilegeMethod]) -> String {
    methods
        .iter()
        .map(|method| method.command())
        .collect::<Vec<_>>()
        .join(", ")
}

fn run_privileged_shell(
    method: PrivilegeMethod,
    shell_command: &str,
) -> io::Result<CommandOutcome> {
    let status = command_for_method(method, shell_command).status();
    match status {
        Ok(status) if status.success() => Ok(CommandOutcome::Success),
        Ok(_) => Ok(CommandOutcome::Failure),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(CommandOutcome::NotFound),
        Err(error) => Err(error),
    }
}

fn command_for_method(method: PrivilegeMethod, shell_command: &str) -> Command {
    let mut command = match method {
        PrivilegeMethod::SudoNonInteractive => {
            let mut command = Command::new("sudo");
            command.arg("-n");
            command
        }
        PrivilegeMethod::SudoInteractive => Command::new("sudo"),
        PrivilegeMethod::Pkexec => Command::new("pkexec"),
    };
    command.args(["sh", "-c", shell_command]);
    command
}

impl PrivilegeMethod {
    fn command(&self) -> &'static str {
        match self {
            PrivilegeMethod::SudoNonInteractive | PrivilegeMethod::SudoInteractive => "sudo",
            PrivilegeMethod::Pkexec => "pkexec",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CommandOutcome, PrivilegeMethod, append_hosts_entry_with_runner, escalation_methods,
    };

    #[test]
    fn escalation_methods_include_interactive_sudo_and_pkexec_on_linux_tty() {
        assert_eq!(
            escalation_methods(true, true),
            vec![
                PrivilegeMethod::SudoNonInteractive,
                PrivilegeMethod::SudoInteractive,
                PrivilegeMethod::Pkexec
            ]
        );
    }

    #[test]
    fn escalation_methods_skip_interactive_sudo_without_tty() {
        assert_eq!(
            escalation_methods(false, true),
            vec![PrivilegeMethod::SudoNonInteractive, PrivilegeMethod::Pkexec]
        );
    }

    #[test]
    fn runner_tries_interactive_sudo_after_non_interactive_failure() {
        let mut calls = Vec::new();
        let mut runner = |method: PrivilegeMethod, _: &str| {
            calls.push(method);
            Ok(match method {
                PrivilegeMethod::SudoNonInteractive => CommandOutcome::Failure,
                PrivilegeMethod::SudoInteractive => CommandOutcome::Success,
                PrivilegeMethod::Pkexec => CommandOutcome::Failure,
            })
        };

        append_hosts_entry_with_runner("app.berth", true, true, &mut runner).expect("update works");
        assert_eq!(
            calls,
            vec![
                PrivilegeMethod::SudoNonInteractive,
                PrivilegeMethod::SudoInteractive
            ]
        );
    }

    #[test]
    fn runner_uses_pkexec_on_linux_when_sudo_is_unavailable() {
        let mut calls = Vec::new();
        let mut runner = |method: PrivilegeMethod, _: &str| {
            calls.push(method);
            Ok(match method {
                PrivilegeMethod::SudoNonInteractive | PrivilegeMethod::SudoInteractive => {
                    CommandOutcome::NotFound
                }
                PrivilegeMethod::Pkexec => CommandOutcome::Success,
            })
        };

        append_hosts_entry_with_runner("app.berth", false, true, &mut runner)
            .expect("update works");
        assert_eq!(
            calls,
            vec![PrivilegeMethod::SudoNonInteractive, PrivilegeMethod::Pkexec]
        );
    }

    #[test]
    fn runner_reports_non_interactive_hint_when_no_tty() {
        let mut runner = |_: PrivilegeMethod, _: &str| Ok(CommandOutcome::Failure);

        let err = append_hosts_entry_with_runner("app.berth", false, false, &mut runner)
            .expect_err("expected failure");
        assert!(err.to_string().contains("no interactive terminal detected"));
    }

    #[test]
    fn runner_rejects_invalid_hostname_before_running_commands() {
        let mut called = false;
        let mut runner = |_: PrivilegeMethod, _: &str| {
            called = true;
            Ok(CommandOutcome::Success)
        };

        let err = append_hosts_entry_with_runner("bad host", true, true, &mut runner)
            .expect_err("invalid host");
        assert!(err.to_string().contains("invalid hostname"));
        assert!(!called, "command runner should not be called");
    }
}
