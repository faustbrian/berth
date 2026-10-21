//! The dump a restore reads, opened and checked before anything is changed.

use anyhow::{Context, Result};
use flate2::read::MultiGzDecoder;
use std::io::{Cursor, Read};
use std::path::Path;

/// Where a restore's dump came from, for the messages about it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DumpOrigin {
    /// `berth restore --file`.
    FileFlag,
    /// `berth restore` reading stdin.
    Stdin,
    /// A service's `seed_file`, applied by `berth up --seed` or `apply`.
    SeedFile,
}

impl DumpOrigin {
    fn check_path(self) -> &'static str {
        match self {
            Self::FileFlag => "check the `--file` path",
            Self::Stdin => "check the command feeding stdin",
            Self::SeedFile => "check the service's `seed_file` in the project file",
        }
    }
}

/// How a dump is read: as gzip or not, and why.
#[derive(Clone, Copy, Debug)]
pub(super) struct DumpRead<'a> {
    /// `--gzip` was passed.
    pub(super) gzip_flag: bool,
    /// The file name ends in `.gz` (a file only).
    pub(super) gz_name: bool,
    pub(super) origin: DumpOrigin,
    /// The target is a shared server's PostgreSQL tenant, which cannot
    /// restore TimescaleDB hypertables (their chunks live in the extension's
    /// schemas).
    pub(super) shared_postgres: bool,
    /// The target is PostgreSQL, whose client the line tap follows.
    pub(super) postgres: bool,
    /// The service's database, which a MySQL dump may `USE`.
    pub(super) database: &'a str,
}

impl DumpRead<'_> {
    fn gzip(self) -> bool {
        self.gzip_flag || self.gz_name
    }
}

/// A failure reading the dump itself, as opposed to the database rejecting
/// its SQL: the restore reports it first, since the client only saw a
/// dump cut short.
#[derive(Debug)]
pub(super) struct DumpReadError {
    /// What the dump looked like when the read failed.
    pub(super) cause: String,
    /// The whole message, for a restore under way.
    pub(super) message: String,
}

impl std::fmt::Display for DumpReadError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for DumpReadError {}

/// How many leading bytes tell the formats apart (`ustar` sits at 257).
const SNIFF_LEN: u64 = 512;

/// Opens the dump at `file_path` and checks it as [`open_dump`] does, after
/// refusing a missing path, a directory or other non-file, and an empty
/// file. A gzip file is decompressed once in full first, so one damaged or
/// cut short past its start fails before anything is dropped. Returns its
/// size and the reader to restore from.
pub(super) fn check_dump_file(
    file_path: &Path,
    read: DumpRead<'_>,
) -> Result<(u64, Box<dyn Read>)> {
    let fix = read.origin.check_path();
    let shown = file_path.display().to_string();
    let metadata =
        std::fs::metadata(file_path).with_context(|| format!("no SQL dump at {shown}; {fix}"))?;
    if metadata.is_dir() {
        anyhow::bail!("{shown} is not a dump file (it is a directory); {fix}");
    }
    if !metadata.is_file() {
        // A pipe (`--file <(gunzip -c …)`) or device: the file is read
        // twice, checked in full before anything is dropped, then restored.
        anyhow::bail!(
            "{shown} is not a regular file (a pipe or device), and a dump file is read twice (checked \
             in full before anything changes); send a stream on stdin instead (`berth restore \
             --service <db> < dump.sql`)"
        );
    }
    if metadata.len() == 0 {
        anyhow::bail!(
            "{shown} is empty (0 bytes); make the dump again from the database it came from \
             (`berth dump --service <db> --file <path>`, run in that project)"
        );
    }
    // Opened once: every pass reads the same file, even if another
    // process replaces the path meanwhile.
    let file = std::fs::File::open(file_path).with_context(|| format!("failed to open {shown}"))?;
    Ok((metadata.len(), check_open_dump(&file, read, &shown)?))
}

/// Reads the dump in `file` in full and refuses it, changing nothing, when
/// it is damaged, cut short, or holds what the target cannot take; then
/// returns a reader of it from its start.
pub(super) fn check_open_dump(
    file: &std::fs::File,
    read: DumpRead<'_>,
    shown: &str,
) -> Result<Box<dyn Read>> {
    let reopen = || -> Result<Box<dyn Read>> {
        let mut clone = file
            .try_clone()
            .with_context(|| format!("failed to read {shown}"))?;
        std::io::Seek::seek(&mut clone, std::io::SeekFrom::Start(0))
            .with_context(|| format!("failed to read {shown}"))?;
        Ok(Box::new(std::io::BufReader::new(clone)))
    };
    // The whole dump is read once first, so its damage, a missing end and
    // what it holds are known before anything is dropped.
    let (mut tap, statements) =
        super::statement_lines::LineTap::new(open_dump(reopen()?, read, shown)?, read.postgres);
    tap.expect_database(read.database);
    std::io::copy(&mut tap, &mut std::io::sink()).map_err(|error| {
        match error
            .get_ref()
            .and_then(|inner| inner.downcast_ref::<super::statement_lines::DumpRefused>())
        {
            Some(refused) => anyhow::anyhow!("{shown} {refused}; nothing was changed"),
            None => anyhow::anyhow!(
                "{shown} is damaged or cut short ({}); make the dump again; nothing was changed",
                dump_read_error_text(&error)
            ),
        }
    })?;
    let statements = statements
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if statements.made_by_dump_tool && !statements.complete {
        anyhow::bail!(
            "{shown} is cut short: its dump tool's end-of-dump line is missing, so it would \
             restore part of the database; make the dump again; nothing was changed"
        );
    }
    if read.shared_postgres && statements.hypertables {
        anyhow::bail!("{shown} {HYPERTABLES}; nothing was changed");
    }
    drop(statements);
    open_dump(reopen()?, read, shown)
}

/// Why a shared tenant cannot restore a dump with hypertables, and the fix.
pub(super) const HYPERTABLES: &str = "holds TimescaleDB hypertables, which a shared server's \
    tenant cannot restore (their chunks live in the extension's own schemas): set \
    `isolation: dedicated` on the service, start it, and restore again";

/// The message of a read error, without the restore's partial-restore
/// wording a [`DumpReadError`] carries (nothing was changed yet).
fn dump_read_error_text(error: &std::io::Error) -> String {
    error
        .get_ref()
        .and_then(|inner| inner.downcast_ref::<DumpReadError>())
        .map_or_else(|| error.to_string(), |read| read.cause.clone())
}

/// Reads the start of `raw` and refuses, before `--reset` drops anything:
/// a format the database client cannot read (a pg_dump archive, or zstd,
/// bzip2 or xz compression), gzip read as plain SQL or the reverse, and a
/// dump with no SQL in it (empty input, or gzip of nothing, as when the
/// command that made it failed). Returns a reader of the dump's SQL,
/// starting with what was read; read errors past that are [`DumpReadError`].
pub(super) fn open_dump(
    mut raw: Box<dyn Read>,
    read: DumpRead<'_>,
    what: &str,
) -> Result<Box<dyn Read>> {
    let mut head = Vec::with_capacity(SNIFF_LEN as usize);
    (&mut raw)
        .take(SNIFF_LEN)
        .read_to_end(&mut head)
        .with_context(|| format!("failed to read {what}"))?;
    if head.is_empty() {
        anyhow::bail!(
            "{what} is empty, so there is nothing to restore (did the command that made it \
             fail?); nothing was changed"
        );
    }
    let is_gzip = head.starts_with(&[0x1f, 0x8b]);
    if let Some(problem) = unreadable_format(&head) {
        anyhow::bail!("{what} {problem}; nothing was changed");
    }
    if read.gzip() && !is_gzip {
        let fix = match (read.gzip_flag, read.gz_name) {
            (true, true) => "drop `--gzip` and rename it without .gz",
            (true, false) => "drop `--gzip`",
            _ if read.origin == DumpOrigin::SeedFile => {
                "rename it without .gz (and the `seed_file` with it), or compress it"
            }
            _ => "rename it without .gz, or compress it",
        };
        anyhow::bail!("{what} is not gzip-compressed, but it is read as gzip; {fix}");
    }
    if !read.gzip() && is_gzip {
        let fix = match read.origin {
            DumpOrigin::FileFlag => "pass `--gzip`, or give it a .gz name",
            DumpOrigin::Stdin => "pass `--gzip`",
            DumpOrigin::SeedFile => "give the `seed_file` a .gz name",
        };
        anyhow::bail!("{what} is gzip-compressed, but it is read as plain SQL; {fix}");
    }
    let raw = Cursor::new(head).chain(raw);
    let sql: Box<dyn Read> = if read.gzip() {
        Box::new(MultiGzDecoder::new(raw))
    } else {
        Box::new(raw)
    };
    // Read before the reset, untagged: damage here changed nothing yet.
    let mut sql = sql;
    let mut start = Vec::with_capacity(8192);
    (&mut sql)
        .take(8192)
        .read_to_end(&mut start)
        .map_err(|error| {
            anyhow::anyhow!("{what} is damaged or cut short ({error}); nothing was changed")
        })?;
    let sql = TagReadErrors {
        inner: sql,
        what: what.to_owned(),
    };
    // SQL is text: a NUL byte means binary data (or UTF-16 text), which
    // the client would reject only after `--reset` dropped everything.
    if start.contains(&0) {
        anyhow::bail!(
            "{what} is not a SQL text dump (it holds binary data, or is UTF-16 text): restore \
             a plain SQL dump (`berth dump` writes one), or gzip of one; nothing was changed"
        );
    }
    if start.is_empty() {
        anyhow::bail!(
            "{what} holds no SQL (it is empty once decompressed), so there is nothing to \
             restore; did the command that made it fail? Nothing was changed"
        );
    }
    Ok(Box::new(Cursor::new(start).chain(sql)))
}

/// What the client cannot read, by the dump's leading bytes.
fn unreadable_format(head: &[u8]) -> Option<&'static str> {
    if head.starts_with(b"PGDMP") || head.get(257..262) == Some(b"ustar") {
        return Some(
            "is a pg_dump archive (custom or tar format), not SQL; restore it with \
             `pg_restore`, or make a plain dump (`berth dump` writes one)",
        );
    }
    if head.starts_with(b"PK\x03\x04") {
        return Some(
            "is a zip archive; extract the dump first (`unzip`): berth reads plain SQL or gzip",
        );
    }
    if head.starts_with(&[b'7', b'z', 0xbc, 0xaf, 0x27, 0x1c]) {
        return Some(
            "is a 7z archive; extract the dump first (`7z x`): berth reads plain SQL or gzip",
        );
    }
    if head.starts_with(&[0x04, 0x22, 0x4d, 0x18]) {
        return Some(
            "is lz4-compressed; decompress it first (`lz4 -d`): berth reads plain SQL or gzip",
        );
    }
    if head.starts_with(&[0x1f, 0x9d]) {
        return Some(
            "is compress-compressed (.Z); decompress it first (`uncompress`): berth reads plain SQL \
             or gzip",
        );
    }
    if head.starts_with(b"LZIP") {
        return Some(
            "is lzip-compressed; decompress it first (`lzip -d`): berth reads plain SQL or gzip",
        );
    }
    if head.starts_with(&[0x28, 0xb5, 0x2f, 0xfd]) {
        Some("is zstd-compressed; decompress it first (`zstd -d`): berth reads plain SQL or gzip")
    } else if head.starts_with(b"BZh") {
        Some("is bzip2-compressed; decompress it first (`bunzip2`): berth reads plain SQL or gzip")
    } else if head.starts_with(&[0xfd, b'7', b'z', b'X', b'Z', 0x00]) {
        Some("is xz-compressed; decompress it first (`unxz`): berth reads plain SQL or gzip")
    } else {
        None
    }
}

/// The dump's own read failure in `error`'s chain, if that is what it is.
pub(super) fn dump_read_error(error: &anyhow::Error) -> Option<&DumpReadError> {
    error.chain().find_map(|cause| {
        cause
            .downcast_ref::<std::io::Error>()
            .and_then(|io| io.get_ref())
            .and_then(|inner| inner.downcast_ref::<DumpReadError>())
            .or_else(|| cause.downcast_ref::<DumpReadError>())
    })
}

/// Marks read errors as the dump's own ([`DumpReadError`]).
struct TagReadErrors {
    inner: Box<dyn Read>,
    what: String,
}

impl Read for TagReadErrors {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.inner.read(buf).map_err(|error| {
            std::io::Error::other(DumpReadError {
                cause: error.to_string(),
                message: format!(
                    "{} is damaged or cut short ({error}); the database holds a partial restore, \
                     so restore a good dump with `--reset`",
                    self.what
                ),
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{DumpOrigin, DumpRead, check_dump_file, open_dump};
    use std::io::{Read, Write};

    fn read(gzip_flag: bool, gz_name: bool, origin: DumpOrigin) -> DumpRead<'static> {
        DumpRead {
            gzip_flag,
            gz_name,
            origin,
            shared_postgres: false,
            postgres: true,
            database: "app",
        }
    }

    fn gzip_of(bytes: &[u8]) -> Vec<u8> {
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(bytes).expect("compress");
        encoder.finish().expect("finish")
    }

    fn opened(bytes: Vec<u8>, read: DumpRead<'_>) -> Result<String, String> {
        let mut sql = String::new();
        open_dump(Box::new(std::io::Cursor::new(bytes)), read, "the dump")
            .map_err(|error| format!("{error:#}"))?
            .read_to_string(&mut sql)
            .expect("read");
        Ok(sql)
    }

    #[test]
    fn a_dump_is_refused_before_the_reset_unless_it_holds_sql_in_its_format() {
        let plain = read(false, false, DumpOrigin::FileFlag);
        let flagged = read(true, false, DumpOrigin::FileFlag);
        assert_eq!(
            opened(b"select 1;".to_vec(), plain).expect("plain"),
            "select 1;"
        );
        assert_eq!(
            opened(gzip_of(b"select 1;"), flagged).expect("gzip"),
            "select 1;"
        );
        // Concatenated gzip members come through whole.
        let mut two = gzip_of(b"select 1;");
        two.extend(gzip_of(b"select 2;"));
        assert_eq!(
            opened(two, flagged).expect("two members"),
            "select 1;select 2;"
        );
        let gzip_as_plain = opened(gzip_of(b"x"), plain).expect_err("gzip as plain");
        assert!(
            gzip_as_plain.contains("pass `--gzip`, or give it a .gz name"),
            "{gzip_as_plain}"
        );
        let seed = opened(gzip_of(b"x"), read(false, false, DumpOrigin::SeedFile))
            .expect_err("seed gzip as plain");
        assert!(seed.contains("`seed_file` a .gz name"), "{seed}");
        let plain_as_gzip = opened(b"select 1;".to_vec(), flagged).expect_err("plain as gzip");
        assert!(plain_as_gzip.ends_with("drop `--gzip`"), "{plain_as_gzip}");
        let both = opened(
            b"select 1;".to_vec(),
            read(true, true, DumpOrigin::FileFlag),
        )
        .expect_err("flag and name");
        assert!(
            both.contains("drop `--gzip` and rename it without .gz"),
            "{both}"
        );
        let named_gz = opened(
            b"select 1;".to_vec(),
            read(false, true, DumpOrigin::SeedFile),
        )
        .expect_err("plain named .gz");
        assert!(named_gz.contains("rename it without .gz") && !named_gz.contains("--gzip"));
        let empty_gzip = opened(gzip_of(b""), flagged).expect_err("gzip of nothing");
        assert!(
            empty_gzip.contains("empty once decompressed"),
            "{empty_gzip}"
        );
        for empty in [plain, flagged] {
            let message = opened(Vec::new(), empty).expect_err("empty");
            assert!(
                message.contains("is empty, so there is nothing to restore"),
                "{message}"
            );
        }
        for (bytes, expected) in [
            (b"PGDMP\x01".to_vec(), "pg_restore"),
            (vec![0x28, 0xb5, 0x2f, 0xfd, 0], "zstd -d"),
            (b"BZh91AY".to_vec(), "bunzip2"),
            (vec![0xfd, b'7', b'z', b'X', b'Z', 0], "unxz"),
            (b"PK\x03\x04".to_vec(), "unzip"),
            (vec![b'7', b'z', 0xbc, 0xaf, 0x27, 0x1c], "7z x"),
            (vec![0x04, 0x22, 0x4d, 0x18], "lz4 -d"),
            (vec![0x1f, 0x9d, 0x90], "uncompress"),
            (b"LZIP\x01".to_vec(), "lzip -d"),
        ] {
            let message = opened(bytes, plain).expect_err("unreadable format");
            assert!(message.contains(expected), "{message}");
        }
        let mut tar = vec![0_u8; 512];
        tar[257..262].copy_from_slice(b"ustar");
        assert!(opened(tar, plain).expect_err("tar").contains("pg_restore"));
        // More than one chunk comes through whole and in order.
        let long = "select 1;\n".repeat(2000);
        assert_eq!(
            opened(gzip_of(long.as_bytes()), flagged).expect("long"),
            long
        );
    }

    #[test]
    fn a_dump_file_must_be_a_whole_non_empty_file() {
        let dir =
            crate::docker::unique_test_path(crate::test_support::temp_root().join("dump-file"));
        std::fs::create_dir_all(&dir).expect("dir");
        let check = |name: &str, read| {
            check_dump_file(&dir.join(name), read)
                .map(|(size, _)| size)
                .map_err(|error| format!("{error:#}"))
        };
        let plain = read(false, false, DumpOrigin::FileFlag);
        let missing = check("nope.sql", plain).expect_err("missing");
        assert!(missing.contains("check the `--file` path"), "{missing}");
        let directory = check(".", plain).expect_err("directory");
        assert!(directory.contains("is not a dump file"), "{directory}");
        // A pipe cannot be read twice: it is sent to stdin instead.
        let status = std::process::Command::new("mkfifo")
            .arg(dir.join("pipe.sql"))
            .status()
            .expect("mkfifo");
        assert!(status.success());
        let pipe = check("pipe.sql", plain).expect_err("pipe");
        assert!(pipe.contains("send a stream on stdin instead"), "{pipe}");
        std::fs::write(dir.join("empty.sql"), "").expect("empty");
        // The dump comes from its source database: a command for this
        // restore's own instance would dump the wrong one.
        let empty = check("empty.sql", plain).expect_err("empty");
        assert!(
            empty
                .contains("is empty (0 bytes); make the dump again from the database it came from"),
            "{empty}"
        );
        assert!(!empty.contains("dump the database again"), "{empty}");
        std::fs::write(dir.join("ok.sql"), "select 1;").expect("dump");
        assert_eq!(check("ok.sql", plain).expect("ok"), 9);
        // Cut short within its first 8 KiB, or past it: caught before
        // anything is dropped, and said so.
        let short = gzip_of(b"select 1;\n");
        std::fs::write(dir.join("short.sql.gz"), &short[..short.len() - 6]).expect("short");
        let short =
            check("short.sql.gz", read(false, true, DumpOrigin::FileFlag)).expect_err("short");
        assert!(short.contains("nothing was changed"), "{short}");
        assert!(!short.contains("partial restore"), "{short}");
        let long = gzip_of("select 1;\n".repeat(50_000).as_bytes());
        std::fs::write(dir.join("cut.sql.gz"), &long[..long.len() - 100]).expect("cut");
        let cut = check("cut.sql.gz", read(false, true, DumpOrigin::FileFlag)).expect_err("cut");
        assert!(cut.contains("damaged or cut short"), "{cut}");
        let seed = check("nope.sql", read(false, false, DumpOrigin::SeedFile)).expect_err("seed");
        assert!(seed.contains("`seed_file` in the project file") && !seed.contains("--file"));
        std::fs::remove_dir_all(&dir).expect("cleanup");
    }

    #[test]
    fn only_a_postgres_dump_is_read_as_psql_reads_it() {
        let dir =
            crate::docker::unique_test_path(crate::test_support::temp_root().join("dump-drivers"));
        std::fs::create_dir_all(&dir).expect("dir");
        // mysqldump escapes a quote with `\'` and a backslash with `\\`: read
        // as PostgreSQL, the `\\c` after it would be psql's `\connect`.
        let mysql = "-- MySQL dump 10.13  Distrib 8.4.0\n\
                     INSERT INTO t VALUES (1,'O\\'Brien','use \\\\c here');\n\
                     -- Dump completed on 2026-10-21 12:00:00\n";
        std::fs::write(dir.join("mysql.sql"), mysql).expect("mysql dump");
        let mysql_read = DumpRead {
            postgres: false,
            ..read(false, false, DumpOrigin::FileFlag)
        };
        check_dump_file(&dir.join("mysql.sql"), mysql_read).expect("a valid mysqldump");
        std::fs::remove_dir_all(&dir).expect("cleanup");
    }
}
