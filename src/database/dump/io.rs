//! database dump io module.
//!
//! Writes a dump to a file or stdout as it streams, never holding it whole.

use anyhow::{Context, Result};
use flate2::Compression;
use flate2::write::GzEncoder;
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Fills `path` with what `fill` writes (gzip-compressed when `gzip`),
/// through a partial file beside it that replaces `path` only once `fill`
/// succeeded: a failed dump never leaves a truncated one behind. Returns
/// what `fill` returned (its raw byte count).
pub(super) fn write_dump_file(
    path: &Path,
    gzip: bool,
    fill: impl FnOnce(&mut dyn Write) -> Result<u64>,
) -> Result<u64> {
    // A pipe or device (`--file /dev/null`, `--file >(gzip > x.gz)`) takes
    // the dump as it streams: nothing can be renamed over it. A directory is
    // refused before anything is dumped.
    if let Ok(metadata) = std::fs::metadata(path)
        && !metadata.is_file()
    {
        if metadata.is_dir() {
            anyhow::bail!(
                "{} is a directory; name the dump file in it (`--file {}/dump.sql`)",
                path.display(),
                path.display()
            );
        }
        let file = std::fs::OpenOptions::new()
            .write(true)
            .open(path)
            .with_context(|| format!("failed to open {} to dump into", path.display()))?;
        return write_through(file, gzip, false, fill);
    }
    // A symlinked target is written through: the link stays a link.
    let target = through_links(path);
    let path = target.as_path();
    remove_dead_partials(path);
    let partial = partial_path(path);
    let written = (|| {
        let file = create_private(&partial).with_context(|| {
            format!(
                "failed to create dump at {}; check that its directory exists and is writable, or \
                 use `--stdout`",
                path.display()
            )
        })?;
        let written = write_through(file, gzip, true, fill)?;
        std::fs::rename(&partial, path)
            .with_context(|| format!("failed to move the dump into place at {}", path.display()))?;
        Ok(written)
    })();
    if written.is_err() {
        drop(std::fs::remove_file(&partial));
    }
    written
}

/// Writes what `fill` writes to stdout (gzip-compressed when `gzip`), once
/// it succeeded: it goes through a private temporary file, so a failed dump
/// never reaches a pipe as a truncated one (`berth dump --stdout | berth
/// restore` would load it).
pub(super) fn write_dump_stdout(
    gzip: bool,
    fill: impl FnOnce(&mut dyn Write) -> Result<u64>,
) -> Result<()> {
    let temp = std::env::temp_dir().join(format!(".berth-dump-{}", unique_suffix()));
    let file = create_private(&temp)
        .with_context(|| format!("failed to create a temporary dump at {}", temp.display()))?;
    let reader = File::open(&temp);
    // Unlinked while open: the data stays readable through both handles, and
    // a killed dump leaves no copy of the database behind.
    std::fs::remove_file(&temp)
        .with_context(|| format!("failed to unlink the temporary dump {}", temp.display()))?;
    let mut reader = reader.with_context(|| format!("failed to read back {}", temp.display()))?;
    write_through(file, gzip, true, fill)?;
    std::io::copy(&mut reader, &mut std::io::stdout().lock())
        .context("Failed to write dump to stdout")?;
    Ok(())
}

/// Writes `fill`'s output into `file`, compressing it when `gzip`, and
/// syncs it to disk when `sync` (a pipe or device cannot be synced).
fn write_through(
    file: File,
    gzip: bool,
    sync: bool,
    fill: impl FnOnce(&mut dyn Write) -> Result<u64>,
) -> Result<u64> {
    let mut buffered = std::io::BufWriter::new(file);
    let written = if gzip {
        let mut encoder = GzEncoder::new(&mut buffered, Compression::default());
        let written = fill(&mut encoder)?;
        encoder.finish().context("Failed to finalize gzip dump")?;
        written
    } else {
        fill(&mut buffered)?
    };
    let file = buffered
        .into_inner()
        .map_err(|error| anyhow::anyhow!("Failed to write dump: {}", error.error()))?;
    if sync {
        file.sync_all().context("Failed to write dump")?;
    }
    Ok(written)
}

/// A dump holds the database's data: only its owner may read it. The file
/// is new, never another run's.
fn create_private(path: &Path) -> std::io::Result<File> {
    use std::os::unix::fs::OpenOptionsExt;
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
}

/// `.<name>.<unique>.berth-partial` beside `path`: one of its own per dump,
/// so two dumps to one path never write into the same file.
fn partial_path(path: &Path) -> PathBuf {
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "dump".to_owned());
    path.with_file_name(format!(".{name}.{}.berth-partial", unique_suffix()))
}

/// What `path` names once its symlinks are followed (to a file that may
/// not exist yet).
fn through_links(path: &Path) -> PathBuf {
    let mut current = path.to_path_buf();
    // As the kernel's own limit on a chain of links.
    for _ in 0..40 {
        let is_link = std::fs::symlink_metadata(&current)
            .is_ok_and(|metadata| metadata.file_type().is_symlink());
        if !is_link {
            break;
        }
        let Ok(next) = std::fs::read_link(&current) else {
            break;
        };
        current = match current.parent() {
            Some(parent) if next.is_relative() => parent.join(next),
            _ => next,
        };
    }
    current
}

/// Removes partial files beside `path` that dead dumps left (killed before
/// they could clean up). A live dump's, named by its running process, stays.
fn remove_dead_partials(path: &Path) {
    let (Some(dir), Some(name)) = (path.parent(), path.file_name()) else {
        return;
    };
    let prefix = format!(".{}.", name.to_string_lossy());
    let Ok(entries) = std::fs::read_dir(if dir.as_os_str().is_empty() {
        Path::new(".")
    } else {
        dir
    }) else {
        return;
    };
    for entry in entries.filter_map(Result::ok) {
        let file = entry.file_name().to_string_lossy().into_owned();
        let Some(pid) = file
            .strip_prefix(&prefix)
            .and_then(|rest| rest.strip_suffix(".berth-partial"))
            // Exactly `<pid>-<count>`: a sibling target such as
            // `backup.2026-10-20.sql` shares the `.backup.` prefix.
            .and_then(|stamp| stamp.split_once('-'))
            .filter(|(pid, count)| {
                [pid, count]
                    .iter()
                    .all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()))
            })
            .and_then(|(pid, _)| pid.parse::<i32>().ok())
        else {
            continue;
        };
        if !crate::home::process_alive(pid) {
            drop(std::fs::remove_file(entry.path()));
        }
    }
}

/// The process ID and a per-process count: no two dumps share it.
fn unique_suffix() -> String {
    static COUNT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    format!(
        "{}-{}",
        std::process::id(),
        COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    )
}

/// Returns whether the path has a `.gz` extension.
pub(super) fn is_gzip_path(path: &Path) -> bool {
    path.extension()
        .and_then(std::ffi::OsStr::to_str)
        .is_some_and(|ext| ext.eq_ignore_ascii_case("gz"))
}

#[cfg(test)]
mod tests {
    use flate2::read::GzDecoder;
    use std::io::Read;
    use std::path::Path;

    use super::{is_gzip_path, write_dump_file};

    /// Whether no partial dump file is left beside `path`.
    fn no_partial_beside(path: &Path) -> bool {
        std::fs::read_dir(path.parent().expect("dir"))
            .expect("read dir")
            .filter_map(Result::ok)
            .all(|entry| {
                !entry
                    .file_name()
                    .to_string_lossy()
                    .ends_with(".berth-partial")
            })
    }

    fn temp_file_path(prefix: &str) -> std::path::PathBuf {
        let dir = crate::test_support::temp_root().join(format!(
            "berth-dump-io-{}-{}",
            std::process::id(),
            crate::test_support::unique_stamp()
        ));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        dir.join(format!("{prefix}.out"))
    }

    fn hello(sink: &mut dyn std::io::Write) -> anyhow::Result<u64> {
        sink.write_all(b"hello")?;
        Ok(5)
    }

    /// This process's `--stdout` temporary dumps in the temp directory.
    fn stdout_temps() -> Vec<String> {
        let prefix = format!(".berth-dump-{}-", std::process::id());
        std::fs::read_dir(std::env::temp_dir())
            .expect("temp dir")
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name.starts_with(&prefix))
            .collect()
    }

    #[test]
    fn a_stdout_dump_keeps_no_copy_on_disk_while_or_after_it_runs() {
        super::write_dump_stdout(false, |sink| {
            assert!(stdout_temps().is_empty(), "unlinked while it is written");
            hello(sink)
        })
        .expect("dump");
        let failed = super::write_dump_stdout(false, |_| anyhow::bail!("dump failed"));
        assert!(failed.is_err());
        assert!(stdout_temps().is_empty(), "{:?}", stdout_temps());
    }

    #[test]
    fn a_directory_target_is_refused_before_anything_is_dumped() {
        let path = temp_file_path("dir");
        let dir = path.parent().expect("dir");
        let error =
            write_dump_file(dir, false, |_| panic!("nothing is dumped")).expect_err("a directory");
        assert!(format!("{error:#}").contains("is a directory"), "{error:#}");
    }

    #[test]
    fn a_pipe_or_device_target_takes_the_dump_as_it_streams() {
        write_dump_file(Path::new("/dev/null"), false, hello).expect("dump to /dev/null");
        let pipe = temp_file_path("fifo");
        let status = std::process::Command::new("mkfifo")
            .arg(&pipe)
            .status()
            .expect("mkfifo");
        assert!(status.success());
        let reader = {
            let pipe = pipe.clone();
            std::thread::spawn(move || std::fs::read_to_string(pipe).expect("read the pipe"))
        };
        write_dump_file(&pipe, false, hello).expect("dump to a pipe");
        assert_eq!(reader.join().expect("reader"), "hello");
        assert!(no_partial_beside(&pipe));
    }

    #[test]
    fn a_sibling_targets_partial_file_is_never_taken_for_a_dead_dumps() {
        let path = temp_file_path("backup");
        let dir = path.parent().expect("dir");
        let name = path
            .file_name()
            .expect("name")
            .to_string_lossy()
            .into_owned();
        // A dump to `<name>.2026-10-20.sql` in progress (its pid is not
        // the `2026` the name starts with).
        let sibling = dir.join(format!(".{name}.2026-10-20.sql.999999999-0.berth-partial"));
        std::fs::write(&sibling, "another dump").expect("sibling partial");
        write_dump_file(&path, false, hello).expect("write");
        assert!(sibling.exists(), "a sibling target's partial file is kept");
    }

    #[test]
    fn write_dump_file_writes_raw_when_not_gzipped() {
        let path = temp_file_path("raw");
        assert_eq!(
            write_dump_file(&path, false, hello).expect("write raw dump"),
            5
        );
        assert_eq!(std::fs::read(&path).expect("read raw file"), b"hello");
        assert!(no_partial_beside(&path));
    }

    #[test]
    fn write_dump_file_writes_gzipped_payload() -> anyhow::Result<()> {
        let path = temp_file_path("gzip");
        write_dump_file(&path, true, hello).expect("write gzip dump");
        let mut decoded = String::new();
        GzDecoder::new(std::fs::File::open(&path)?).read_to_string(&mut decoded)?;
        assert_eq!(decoded, "hello");
        Ok(())
    }

    #[test]
    fn a_failed_dump_leaves_the_old_file_and_no_partial_one() {
        let path = temp_file_path("failed");
        std::fs::write(&path, "previous dump").expect("old dump");
        let error = write_dump_file(&path, false, |sink| {
            sink.write_all(b"half a dump")?;
            anyhow::bail!("pg_dump failed")
        })
        .expect_err("fails");
        assert!(error.to_string().contains("pg_dump failed"), "{error}");
        assert_eq!(
            std::fs::read_to_string(&path).expect("old"),
            "previous dump"
        );
        assert!(no_partial_beside(&path));
    }

    #[test]
    fn two_dumps_to_one_path_never_share_a_partial_file() {
        let path = temp_file_path("concurrent");
        // A second dump starts while the first is still writing.
        let outer = write_dump_file(&path, false, |outer_sink| {
            outer_sink.write_all(b"first ")?;
            write_dump_file(&path, false, |inner_sink| {
                inner_sink.write_all(b"second")?;
                Ok(6)
            })?;
            assert_eq!(std::fs::read_to_string(&path)?, "second");
            outer_sink.write_all(b"dump")?;
            Ok(10)
        })
        .expect("outer");
        assert_eq!(outer, 10);
        assert_eq!(std::fs::read_to_string(&path).expect("read"), "first dump");
        assert!(no_partial_beside(&path));
    }

    #[test]
    fn a_dead_dumps_partial_file_is_cleared_and_a_link_is_written_through() {
        let path = temp_file_path("leftover");
        let dir = path.parent().expect("dir");
        // A pid far above any running process's.
        let dead = dir.join(".real.sql.999999999-0.berth-partial");
        std::fs::write(&dead, "killed dump").expect("dead partial");
        let real = dir.join("real.sql");
        std::os::unix::fs::symlink(&real, &path).expect("link");
        write_dump_file(&path, false, hello).expect("write");
        assert!(!dead.exists(), "a dead dump's partial file is removed");
        assert!(
            std::fs::symlink_metadata(&path)
                .expect("meta")
                .file_type()
                .is_symlink()
        );
        assert_eq!(std::fs::read_to_string(&real).expect("real"), "hello");
    }

    #[test]
    fn a_partial_file_of_a_process_berth_may_not_signal_is_kept() {
        let path = temp_file_path("foreign");
        let dir = path.parent().expect("dir");
        // pid 1 runs as root: signalling it is refused, so it counts as a
        // live dump (another user's, in a shared directory). As root (or in
        // a namespace where pid 1 is ours) the case cannot arise here.
        if rustix::process::Pid::from_raw(1)
            .is_some_and(|pid| rustix::process::test_kill_process(pid).is_ok())
        {
            return;
        }
        let foreign = dir.join(format!(
            ".{}.1-0.berth-partial",
            path.file_name().expect("name").to_string_lossy()
        ));
        std::fs::write(&foreign, "another user's dump").expect("foreign partial");
        write_dump_file(&path, false, hello).expect("write");
        assert!(foreign.exists(), "a live dump's partial file is kept");
        std::fs::remove_file(&foreign).expect("remove");
    }

    #[test]
    fn a_dump_file_is_private() {
        use std::os::unix::fs::PermissionsExt;
        let path = temp_file_path("private");
        write_dump_file(&path, false, hello).expect("write");
        let mode = std::fs::metadata(&path).expect("meta").permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[test]
    fn is_gzip_path_checks_case_insensitive_extension() {
        assert!(is_gzip_path(Path::new("backup.sql.gz")));
        assert!(is_gzip_path(Path::new("backup.SQL.GZ")));
        assert!(!is_gzip_path(Path::new("backup.sql")));
        assert!(!is_gzip_path(Path::new("backup")));
    }
}
