//! Whether a process still runs, for state named after its process ID.

/// Whether process `pid` still runs. One berth may not signal (another
/// user's, in a shared directory) is alive: its work may still be going on.
pub(crate) fn process_alive(pid: i32) -> bool {
    let Some(pid) = rustix::process::Pid::from_raw(pid) else {
        return false;
    };
    match rustix::process::test_kill_process(pid) {
        Ok(()) => true,
        Err(error) => error == rustix::io::Errno::PERM,
    }
}
