//! output logger state module.
//!
//! Contains logger state and persistent file sink logic.

use std::sync::Mutex;

use super::Persistence;
use super::entry::LogEntry;
mod file_sink;
use file_sink::{FileState, default_log_dir_path, persist_entry};

pub(super) struct LoggerState {
    quiet: bool,
    file_state: Mutex<FileState>,
}

impl LoggerState {
    pub(super) fn new(quiet: bool) -> Self {
        Self {
            quiet,
            file_state: Mutex::new(FileState {
                directory: default_log_dir_path(),
                day: None,
                file: None,
            }),
        }
    }

    pub(super) fn emit_terminal(&self, entry: &LogEntry) {
        if self.quiet && !entry.level.visible_when_quiet() {
            return;
        }
        eprintln!("{}", super::terminal_line_for_stderr(entry));
    }

    pub(super) fn persist(&self, entry: &LogEntry) {
        if !matches!(entry.persistence, Persistence::Persistent) {
            return;
        }

        persist_entry(&self.file_state, entry);
    }
}
