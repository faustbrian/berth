//! Persistent file sink internals for logger state.

use std::fs::{File, OpenOptions, create_dir_all};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

use crate::output::entry::LogEntry;

pub(super) struct FileState {
    pub(super) directory: Option<PathBuf>,
    pub(super) day: Option<String>,
    pub(super) file: Option<File>,
}

pub(super) fn default_log_dir_path() -> Option<PathBuf> {
    let Ok(home) = std::env::var("HOME") else {
        return None;
    };
    Some(PathBuf::from(home).join(".config/berth/logs"))
}

pub(super) fn persist_entry(file_state: &Mutex<FileState>, entry: &LogEntry) {
    let Ok(mut state) = file_state.lock() else {
        return;
    };
    let Some(directory) = state.directory.clone() else {
        return;
    };

    let day = entry.timestamp.date().to_string();
    ensure_log_file_for_day(&mut state, &directory, &day);

    if let Some(file) = state.file.as_mut() {
        drop(writeln!(file, "{}", entry.render_file_line()));
    }
}

fn ensure_log_file_for_day(state: &mut FileState, directory: &PathBuf, day: &str) {
    if state.day.as_deref() == Some(day) && state.file.is_some() {
        return;
    }

    if create_dir_all(directory).is_err() {
        return;
    }
    let file_path = directory.join(format!("{day}.log"));
    let Ok(file) = OpenOptions::new().create(true).append(true).open(file_path) else {
        return;
    };
    state.file = Some(file);
    state.day = Some(day.to_owned());
}
