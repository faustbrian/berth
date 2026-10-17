//! Renders log lines for stderr.

use std::io::IsTerminal;

use super::entry::LogEntry;
use super::normalize::strip_ansi_codes;

/// Renders a log line for stderr, without color unless stderr is a terminal.
///
/// The color library decides from stdout, but log lines go to stderr, so a
/// redirected stderr (`berth up 2>up.log`) would otherwise receive escape
/// codes whenever stdout is a terminal.
pub(super) fn terminal_line_for_stderr(entry: &LogEntry) -> String {
    let line = entry.render_terminal_line();
    if std::io::stderr().is_terminal() {
        line
    } else {
        strip_ansi_codes(&line)
    }
}
