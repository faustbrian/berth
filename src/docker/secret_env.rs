//! Keeps secrets out of docker's process arguments.
//!
//! Every local user can read a process's arguments (`ps`), so a secret
//! `-e NAME=value` is passed as `-e NAME` and its value is set in the docker
//! CLI's own environment, which only the invoking user can read. Docker and
//! Podman then copy it into the container exactly as before.

use anyhow::Result;
use std::process::Command;

/// Subcommands whose `-e`/`--env` options set a container's environment.
const ENV_SUBCOMMANDS: [&str; 3] = ["run", "create", "exec"];

/// A `docker` process for `args`, with secret environment values moved out
/// of its arguments.
///
/// # Errors
///
/// Returns an error when one command sets the same secret variable to two
/// different values, which would make the moved value ambiguous.
pub(crate) fn docker_process(args: &[&str]) -> Result<Command> {
    let mut command = Command::new(super::docker_command());
    if !args
        .first()
        .is_some_and(|sub| ENV_SUBCOMMANDS.contains(sub))
    {
        command.args(args);
        return Ok(command);
    }
    let mut moved: Vec<(&str, &str)> = Vec::new();
    let mut previous = "";
    // Options end at the first positional argument (the image for `run` and
    // `create`, the container for `exec`); what follows is the command the
    // container runs, passed through exactly.
    let mut in_options = true;
    command.arg(args[0]);
    for arg in &args[1..] {
        let inline = arg
            .strip_prefix("--env=")
            .or_else(|| arg.strip_prefix("-e="));
        let separate = (previous == "-e" || previous == "--env").then_some(*arg);
        let assignment = in_options
            .then(|| separate.or(inline))
            .flatten()
            .and_then(|value| value.split_once('='))
            .filter(|(name, value)| {
                is_env_name(name) && super::redact::is_secret_assignment(name, value)
            });
        match assignment {
            Some((name, value)) => {
                if moved
                    .iter()
                    .any(|(known, other)| *known == name && *other != value)
                {
                    anyhow::bail!("docker command sets {name} twice to different values");
                }
                moved.push((name, value));
                if inline.is_some() {
                    command.arg(format!("--env={name}"));
                } else {
                    command.arg(name);
                }
            }
            None => {
                command.arg(arg);
            }
        }
        if in_options {
            let is_value = takes_value(previous);
            if !is_value && !arg.starts_with('-') {
                in_options = false;
            }
            // `--` ends options too; the next argument is positional.
            previous = if *arg == "--" || is_value { "" } else { arg };
        }
    }
    for (name, value) in moved {
        command.env(name, value);
    }
    Ok(command)
}

/// Whether docker option `flag` consumes the next argument. Boolean options
/// stand alone; every other option without an inline `=value` takes one.
fn takes_value(flag: &str) -> bool {
    const BOOLEAN: [&str; 16] = [
        "--detach",
        "--interactive",
        "--tty",
        "--rm",
        "--init",
        "--privileged",
        "--read-only",
        "--publish-all",
        "--no-healthcheck",
        "--oom-kill-disable",
        "--quiet",
        "--sig-proxy",
        "--disable-content-trust",
        "--use-api-socket",
        "--help",
        "--no-stdin",
    ];
    if !flag.starts_with('-') || flag == "-" || flag.contains('=') {
        return false;
    }
    if let Some(short) = flag.strip_prefix('-').filter(|rest| !rest.starts_with('-')) {
        // Clusters of boolean short flags (`-it`, `-dit`, `-P`, `-q`).
        return !short
            .chars()
            .all(|c| matches!(c, 'd' | 'i' | 't' | 'P' | 'q'));
    }
    !BOOLEAN.contains(&flag)
}

/// A portable environment variable name. Anything else after `-e` (such as
/// the SQL a `mysql -e` inside the container runs) is left alone.
fn is_env_name(name: &str) -> bool {
    let mut bytes = name.bytes();
    bytes
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic() || first == b'_')
        && bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

#[cfg(test)]
mod tests {
    use super::docker_process;

    fn split(args: &[&str]) -> (Vec<String>, Vec<(String, String)>) {
        let command = docker_process(args).expect("command");
        let argv = command
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect();
        let mut env: Vec<(String, String)> = command
            .get_envs()
            .filter_map(|(name, value)| {
                value.map(|value| {
                    (
                        name.to_string_lossy().into_owned(),
                        value.to_string_lossy().into_owned(),
                    )
                })
            })
            .collect();
        env.sort();
        (argv, env)
    }

    #[test]
    fn secret_values_leave_the_arguments_and_ordinary_ones_stay() {
        let (argv, env) = split(&[
            "run",
            "-d",
            "-e",
            "DB_PASSWORD=hunter22hunter",
            "-e",
            "DB_HOST=db",
            "--env",
            "APP_KEY=base64:x",
            "img",
        ]);
        assert_eq!(
            argv,
            [
                "run",
                "-d",
                "-e",
                "DB_PASSWORD",
                "-e",
                "DB_HOST=db",
                "--env",
                "APP_KEY",
                "img"
            ]
        );
        assert_eq!(
            env,
            [
                ("APP_KEY".to_owned(), "base64:x".to_owned()),
                ("DB_PASSWORD".to_owned(), "hunter22hunter".to_owned())
            ]
        );
    }

    #[test]
    fn other_subcommands_and_non_variables_are_untouched() {
        let (argv, env) = split(&["logs", "-e", "TOKEN=abc"]);
        assert_eq!(argv, ["logs", "-e", "TOKEN=abc"]);
        assert!(env.is_empty());
        let (argv, _) = split(&["exec", "c", "mysql", "-e", "SET PASSWORD='x'"]);
        assert_eq!(argv, ["exec", "c", "mysql", "-e", "SET PASSWORD='x'"]);
    }

    #[test]
    fn the_container_command_is_passed_through_exactly() {
        for args in [
            &["exec", "-i", "c", "tool", "-e", "API_TOKEN=x"][..],
            &["exec", "-it", "-e", "A=1", "c", "tool", "-e", "API_TOKEN=x"][..],
            &[
                "run",
                "--rm",
                "-d",
                "--name",
                "n",
                "-v",
                "a:/b",
                "img",
                "cmd",
                "-e",
                "SECRET_KEY=1",
            ][..],
            &["run", "--rm", "--network=n", "img", "--env", "API_TOKEN=x"][..],
        ] {
            let (argv, env) = split(args);
            assert_eq!(argv, args, "{args:?}");
            assert!(env.is_empty(), "{args:?}");
        }
        let (argv, env) = split(&[
            "run",
            "--env=API_TOKEN=abcdefgh",
            "--no-stdin",
            "-e",
            "DB_PASSWORD=secret12",
            "img",
        ]);
        assert_eq!(
            argv,
            [
                "run",
                "--env=API_TOKEN",
                "--no-stdin",
                "-e",
                "DB_PASSWORD",
                "img"
            ]
        );
        assert_eq!(env.len(), 2);
        let (argv, _) = split(&[
            "run",
            "--rm",
            "--name",
            "n",
            "-e",
            "DB_PASSWORD=secret12",
            "img",
        ]);
        assert_eq!(
            argv,
            ["run", "--rm", "--name", "n", "-e", "DB_PASSWORD", "img"]
        );
    }

    #[test]
    fn conflicting_values_for_one_secret_fail() {
        assert!(
            docker_process(&["run", "-e", "API_TOKEN=one", "-e", "API_TOKEN=two", "img"]).is_err()
        );
        assert!(
            docker_process(&["run", "-e", "API_TOKEN=one", "-e", "API_TOKEN=one", "img"]).is_ok()
        );
    }
}
