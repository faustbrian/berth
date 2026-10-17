//! database dump io module.
//!
//! Contains database dump io logic used by Berth command workflows.

use anyhow::{Context, Result};
use flate2::Compression;
use flate2::write::GzEncoder;
use std::fs::File;
use std::io::Write;
use std::path::Path;

/// Writes dump file to persisted or external state.
pub(super) fn write_dump_file(path: &Path, bytes: &[u8], gzip: bool) -> Result<()> {
    if gzip {
        let file = File::create(path)
            .with_context(|| format!("Failed to create dump at {}", path.display()))?;
        let mut encoder = GzEncoder::new(file, Compression::default());
        encoder
            .write_all(bytes)
            .with_context(|| format!("Failed to write gzip dump to {}", path.display()))?;
        drop(encoder.finish()?);
        return Ok(());
    }

    std::fs::write(path, bytes)
        .with_context(|| format!("Failed to write dump to {}", path.display()))
}

/// Writes dump stdout to persisted or external state.
pub(super) fn write_dump_stdout(bytes: &[u8], gzip: bool) -> Result<()> {
    if gzip {
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(bytes).context("Failed to gzip dump")?;
        let compressed = encoder.finish().context("Failed to finalize gzip dump")?;
        std::io::stdout()
            .lock()
            .write_all(&compressed)
            .context("Failed to write gzip dump to stdout")?;
        return Ok(());
    }

    std::io::stdout()
        .lock()
        .write_all(bytes)
        .context("Failed to write dump to stdout")
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

    fn temp_file_path(prefix: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "berth-dump-io-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock")
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        dir.join(format!("{prefix}.out"))
    }

    #[test]
    fn write_dump_file_writes_raw_when_not_gzipped() {
        let path = temp_file_path("raw");
        write_dump_file(&path, b"hello", false).expect("write raw dump");
        let content = std::fs::read(&path).expect("read raw file");
        assert_eq!(content, b"hello");
    }

    #[test]
    fn write_dump_file_writes_gzipped_payload() -> anyhow::Result<()> {
        let path = temp_file_path("gzip");
        write_dump_file(&path, b"hello", true).expect("write gzip dump");

        let mut decoder = GzDecoder::new(std::fs::File::open(&path)?);
        let mut decoded = String::new();
        decoder.read_to_string(&mut decoded)?;
        assert_eq!(decoded, "hello");
        Ok(())
    }

    #[test]
    fn is_gzip_path_checks_case_insensitive_extension() {
        assert!(is_gzip_path(Path::new("backup.sql.gz")));
        assert!(is_gzip_path(Path::new("backup.SQL.GZ")));
        assert!(!is_gzip_path(Path::new("backup.sql")));
        assert!(!is_gzip_path(Path::new("backup")));
    }
}
