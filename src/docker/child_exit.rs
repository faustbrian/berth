//! Exit status of a command berth ran on the caller's behalf.
//!
//! Agents read berth's exit code to decide whether a test run or command
//! failed, so a non-zero exit of the command inside the container must reach
//! berth's own exit code unchanged instead of collapsing into a generic error.

use std::fmt;
use std::process::ExitStatus;

/// Error carrying the exact exit code of a command that ran in a container.
#[derive(Debug)]
pub(crate) struct ChildExitError {
    code: u8,
    message: String,
}

impl ChildExitError {
    /// The process exit code berth should use: the command's own code, or
    /// `128 + signal` when the command was killed by a signal.
    #[must_use]
    pub(crate) const fn code(&self) -> u8 {
        self.code
    }
}

impl fmt::Display for ChildExitError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for ChildExitError {}

impl ChildExitError {
    /// Builds the error for an explicit exit code.
    #[must_use]
    pub(crate) const fn from_code(code: u8, message: String) -> Self {
        Self { code, message }
    }
}

/// A container command berth ran that exited unsuccessfully.
///
/// Low-level helpers return this for every command. Only the entry points
/// that run the user's own command turn it into a [`ChildExitError`] (see
/// [`pass_through_exit_code`]); a failing internal step such as a hook or a
/// compatibility shim stays a berth failure.
#[derive(Debug)]
pub(crate) struct CommandFailedError {
    code: u8,
    message: String,
}

impl fmt::Display for CommandFailedError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for CommandFailedError {}

/// Builds the error for a finished command whose status was not success.
#[must_use]
pub(crate) fn command_failed_error(status: ExitStatus, message: String) -> anyhow::Error {
    anyhow::Error::new(CommandFailedError {
        code: exit_code_of(status),
        message,
    })
}

/// Makes the exit code of the user's own command berth's exit code.
///
/// # Errors
///
/// Returns the original error; when it was caused by the command exiting
/// unsuccessfully, it becomes a [`ChildExitError`] with that exit code.
pub(crate) fn pass_through_exit_code<T>(result: anyhow::Result<T>) -> anyhow::Result<T> {
    result.map_err(|error| {
        let failed = error
            .chain()
            .find_map(|cause| cause.downcast_ref::<CommandFailedError>())
            .map(|failed| (failed.code, format!("{error:#}")));
        match failed {
            Some((code, message)) => anyhow::Error::new(ChildExitError::from_code(code, message)),
            None => error,
        }
    })
}

/// Maps an exit status to a process exit code (`128 + signal` for signals).
#[must_use]
pub(crate) fn exit_code_of(status: ExitStatus) -> u8 {
    use std::os::unix::process::ExitStatusExt;

    if let Some(code) = status.code() {
        return u8::try_from(code & 0xff).unwrap_or(1);
    }
    let signal = status.signal().unwrap_or(0);
    u8::try_from((128 + signal) & 0xff).unwrap_or(1)
}

#[cfg(test)]
mod tests {
    use std::os::unix::process::ExitStatusExt;
    use std::process::ExitStatus;

    use super::{ChildExitError, command_failed_error, exit_code_of, pass_through_exit_code};

    #[test]
    fn exit_codes_and_signals_map_to_process_exit_codes() {
        assert_eq!(exit_code_of(ExitStatus::from_raw(7 << 8)), 7);
        assert_eq!(exit_code_of(ExitStatus::from_raw(255 << 8)), 255);
        // Raw status 9 is "killed by SIGKILL".
        assert_eq!(exit_code_of(ExitStatus::from_raw(9)), 137);
    }

    fn child_code(error: &anyhow::Error) -> Option<u8> {
        error
            .chain()
            .find_map(|cause| cause.downcast_ref::<ChildExitError>())
            .map(ChildExitError::code)
    }

    #[test]
    fn only_user_command_boundaries_pass_the_exit_code_through() {
        let failed = || -> anyhow::Result<()> {
            Err(
                command_failed_error(ExitStatus::from_raw(3 << 8), "tests failed".to_owned())
                    .context("artisan test"),
            )
        };
        let internal = failed().expect_err("internal step");
        assert_eq!(
            child_code(&internal),
            None,
            "internal failures stay berth failures"
        );

        let boundary = pass_through_exit_code(failed()).expect_err("user command");
        assert_eq!(child_code(&boundary), Some(3));
        assert!(boundary.to_string().contains("tests failed"), "{boundary}");
    }
}
