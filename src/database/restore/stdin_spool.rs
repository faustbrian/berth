//! A dump read from stdin, kept in a private file so it is checked in full
//! before anything is changed, as a `--file` dump is.

use anyhow::{Context, Result};
use std::io::Read;

use super::dump_file::{DumpRead, check_open_dump};

/// Copies `stdin` into an unlinked private temporary file, checks it as
/// `check_dump_file` checks a file, and returns a reader of it. A stream
/// refused or broken partway would otherwise reach the client cut short,
/// after `--reset` dropped the database, and psql runs a cut-off last
/// statement at end of input.
pub(super) fn spool_stdin(mut stdin: impl Read, read: DumpRead<'_>) -> Result<Box<dyn Read>> {
    // Unguessable, so another user cannot take the name first in a shared
    // temporary directory (`RandomState` is seeded randomly per process).
    let random = {
        use std::hash::{BuildHasher, Hasher};
        let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
        hasher.write_u64(u64::from(std::process::id()));
        hasher.finish()
    };
    let temp = std::env::temp_dir().join(format!(
        ".berth-restore-{}-{random:016x}",
        std::process::id()
    ));
    let mut file = {
        use std::os::unix::fs::OpenOptionsExt;
        std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temp)
            .with_context(|| {
                format!(
                    "failed to create a temporary copy of stdin at {}",
                    temp.display()
                )
            })?
    };
    // Unlinked while open: the copy stays readable through the handle, and
    // a killed restore leaves no copy of the database behind.
    std::fs::remove_file(&temp)
        .with_context(|| format!("failed to unlink the temporary copy {}", temp.display()))?;
    // A read and a write failure name their own cause: a full temporary
    // directory is not a broken stream.
    let mut buffer = vec![0_u8; 64 * 1024];
    loop {
        let read = match stdin.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => read,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(anyhow::anyhow!("failed to read stdin: {error}")),
        };
        std::io::Write::write_all(&mut file, buffer.get(..read).unwrap_or_default()).with_context(
            || {
                format!(
                    "failed to keep a copy of stdin in {} (a stdin restore needs room there for \
                     the whole dump; set TMPDIR to a directory with room); nothing was changed",
                    std::env::temp_dir().display()
                )
            },
        )?;
    }
    check_open_dump(&file, read, "stdin")
}

#[cfg(test)]
mod tests {
    use super::super::dump_file::{DumpOrigin, DumpRead};
    use super::spool_stdin;
    use std::io::Read;

    fn read() -> DumpRead<'static> {
        DumpRead {
            gzip_flag: false,
            gz_name: false,
            origin: DumpOrigin::FileFlag,
            shared_postgres: false,
            postgres: true,
            database: "app",
        }
    }

    #[test]
    fn stdin_is_checked_in_full_before_anything_changes() {
        // Refused past its first 8 KiB, where a stream would already be
        // half sent: pg_dumpall's `\connect` comes after its roles.
        let late = format!(
            "{}UPDATE accounts SET balance = 0\nWHERE id = 7;\n\\connect prod\n",
            "-- role\n".repeat(2000)
        );
        let refused = spool_stdin(late.as_bytes(), read()).err().expect("refused");
        let refused = format!("{refused:#}");
        assert!(
            refused.contains("switches to another database")
                && refused.contains("nothing was changed"),
            "{refused}"
        );
        let nul = format!("{}SELECT 1;\0\n", "-- x\n".repeat(4000));
        let refused = spool_stdin(nul.as_bytes(), read()).err().expect("refused");
        assert!(format!("{refused:#}").contains("NUL byte"), "{refused:#}");
        // A good dump comes back whole, from its start.
        let mut restored = String::new();
        spool_stdin("SELECT 1;\n".as_bytes(), read())
            .expect("good")
            .read_to_string(&mut restored)
            .expect("read");
        assert_eq!(restored, "SELECT 1;\n");
        // Nothing is left in the temporary directory.
        let left = std::fs::read_dir(std::env::temp_dir())
            .expect("temp")
            .filter_map(Result::ok)
            .filter(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with(&format!(".berth-restore-{}-", std::process::id()))
            })
            .count();
        assert_eq!(left, 0);
    }
}
