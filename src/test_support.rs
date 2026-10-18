//! Helpers shared by unit tests.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// This test process's own scratch root, `$TMPDIR/berth-ut-{pid}`: unit tests
/// create their fixtures under it instead of directly in the system temp
/// directory. The test harness has no teardown hook, so the first call
/// removes the roots of test processes that have exited; at most the last
/// run's fixtures stay behind. Only roots carrying [`MARKER`] are removed,
/// never another tool's directory. The name is short because socket
/// fixtures under it must fit `SUN_LEN`.
const MARKER: &str = ".berth-unit-tests";

pub(crate) fn temp_root() -> &'static Path {
    static ROOT: OnceLock<PathBuf> = OnceLock::new();
    ROOT.get_or_init(|| {
        let system = std::env::temp_dir();
        if let Ok(entries) = std::fs::read_dir(&system) {
            for entry in entries.filter_map(Result::ok) {
                let name = entry.file_name();
                let Some(pid) = name
                    .to_str()
                    .and_then(|name| name.strip_prefix("berth-ut-"))
                    .and_then(|pid| pid.parse::<i32>().ok())
                else {
                    continue;
                };
                if !alive(pid) && entry.path().join(MARKER).is_file() {
                    drop(std::fs::remove_dir_all(entry.path()));
                }
            }
        }
        let root = system.join(format!("berth-ut-{}", std::process::id()));
        std::fs::create_dir_all(&root).expect("test scratch root");
        std::fs::write(root.join(MARKER), b"").expect("test scratch marker");
        root
    })
}

/// Whether a process with this PID exists (one owned by someone else
/// counts).
fn alive(pid: i32) -> bool {
    let Some(pid) = rustix::process::Pid::from_raw(pid) else {
        return false;
    };
    match rustix::process::test_kill_process(pid) {
        Ok(()) => true,
        Err(error) => error == rustix::io::Errno::PERM,
    }
}

#[cfg(test)]
mod tests {
    use super::{alive, temp_root};

    #[test]
    fn the_scratch_root_is_this_processs_and_exists() {
        let root = temp_root();
        assert!(root.is_dir());
        assert!(root.ends_with(format!("berth-ut-{}", std::process::id())));
        assert!(root.join(super::MARKER).is_file());
        assert!(alive(i32::try_from(std::process::id()).expect("pid")));
    }
}
