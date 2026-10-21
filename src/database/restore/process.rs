//! database restore process module.
//!
//! Contains database restore process logic used by Berth command workflows.

use anyhow::{Context, Result};
use std::collections::BTreeSet;
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, ChildStdin};
use std::thread::JoinHandle;

use crate::config::ServiceConfig;

use super::RestoreOptions;
use super::dump_file::DumpOrigin;
use super::psql_errors::PsqlErrors;
use super::statement_lines::{LineTap, StatementLines};

/// How much of the client's stderr is kept for a failure message.
const KEPT_STDERR: usize = 64 * 1024;

/// The restore client, its stdin, and a thread reading its stderr while the
/// dump streams: a client that prints more than a pipe holds (one error per
/// row) would otherwise block, and the restore with it.
pub(super) struct RestoreProcess {
    pub(super) stdin: ChildStdin,
    child: Child,
    stderr: JoinHandle<Stderr>,
    /// Relays the client's stdout, keeping berth's markers out of it.
    stdout: Option<JoinHandle<Markers>>,
    /// What berth sends after the dump and expects back.
    end: DumpEnd,
}

/// berth's own line after the dump: a random token the client must print,
/// and, for MySQL, the database it must still be in. A dump that quits
/// early (`\q`, which ends only psql's reading of the dump, not its later
/// commands) or ends inside an unfinished statement, string, comment or
/// `\if` never prints it.
#[derive(Clone, Debug)]
pub(super) struct DumpEnd {
    token: String,
    client: DumpClient,
}

#[derive(Clone, Debug)]
enum DumpClient {
    Psql,
    Mysql { database: String },
}

/// What the client prints for the token, and for MySQL its database.
const DUMP_END_MARKER: &str = "berth-dump-end ";
const DATABASE_MARKER: &str = "berth-db ";

impl DumpEnd {
    fn for_service(service: &ServiceConfig) -> Result<Self> {
        let mut bytes = [0_u8; 16];
        getrandom::fill(&mut bytes)
            .map_err(|error| anyhow::anyhow!("failed to make a random token: {error}"))?;
        let token = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
        let client = match service.driver {
            crate::config::Driver::Postgres => DumpClient::Psql,
            crate::config::Driver::Mysql => DumpClient::Mysql {
                database: service.database.clone().unwrap_or_else(|| "app".to_owned()),
            },
            other => anyhow::bail!("berth restores PostgreSQL and MySQL databases, not {other:?}"),
        };
        Ok(Self { token, client })
    }

    /// The commands sent after the dump; the newline first ends a last line
    /// the dump left open.
    fn trailer(&self) -> String {
        let token = &self.token;
        match self.client {
            DumpClient::Psql => format!("\n\\echo {DUMP_END_MARKER}{token}\n"),
            DumpClient::Mysql { .. } => format!(
                "\nSELECT '{DUMP_END_MARKER}{token}' AS berth;\n\
                 SELECT CONCAT('{DATABASE_MARKER}', DATABASE()) AS berth;\n"
            ),
        }
    }
}

/// What the stderr thread kept: its start, and psql's errors by kind.
#[derive(Default)]
struct Stderr {
    start: Vec<u8>,
    errors: PsqlErrors,
    /// Reading the client's stderr failed: errors after it are unknown.
    read_error: Option<String>,
}

pub(super) fn start_restore_process(service: &ServiceConfig) -> Result<RestoreProcess> {
    let end = DumpEnd::for_service(service)?;
    let child =
        crate::docker::exec_piped(service, false).context("Failed to start restore process")?;
    RestoreProcess::new(child, end)
}

impl RestoreProcess {
    /// Sends berth's line after the whole dump (see [`DumpEnd`]).
    ///
    /// # Errors
    ///
    /// Returns an error when the client's stdin cannot be written.
    pub(super) fn send_end(&mut self) -> Result<()> {
        self.stdin
            .write_all(self.end.trailer().as_bytes())
            .context("Failed to write to database process")
    }

    fn new(mut child: Child, end: DumpEnd) -> Result<Self> {
        let stdin = child.stdin.take().context("Failed to open stdin pipe")?;
        let stderr = child.stderr.take().context("Failed to open stderr pipe")?;
        let stdout = child.stdout.take().map(|stdout| {
            std::thread::spawn(move || {
                if crate::output::stdout_reserved() {
                    relay_stdout(stdout, std::io::stderr())
                } else {
                    relay_stdout(stdout, std::io::stdout())
                }
            })
        });
        Ok(Self {
            stdin,
            child,
            stderr: std::thread::spawn(move || read_stderr(stderr)),
            stdout,
            end,
        })
    }
}

/// What berth's own commands around the dump printed
/// (`docker/exec/psql_restore.rs`): the first start marker (the dump runs
/// after it) and the last end markers (it runs before them).
#[derive(Debug, Default, PartialEq, Eq)]
struct Markers {
    start: Option<String>,
    end: Option<String>,
    rollback: Option<String>,
    /// The last token printed after the dump, and MySQL's database then.
    dump_end: Option<String>,
    database: Option<String>,
    /// Reading the client's stdout failed: what came after is unknown.
    read_error: Option<String>,
}

/// The longest stdout line berth looks at for a marker; longer ones are the
/// dump's own and are passed on as they come.
const MARKER_LINE: usize = 256;

/// Copies the client's stdout to `to`, as before, but for berth's marker
/// lines, which it keeps. Copying stops at a write error (berth's stdout
/// closed), but reading goes on so the client never blocks.
fn relay_stdout(from: impl Read, mut to: impl Write) -> Markers {
    let mut reader = BufReader::new(from);
    let mut markers = Markers::default();
    let mut writing = true;
    let mut line: Vec<u8> = Vec::new();
    // The current line is longer than any marker: passed on as it comes.
    let mut long = false;
    let mut after_marker = false;
    let mut write = |bytes: &[u8], writing: &mut bool| {
        if *writing && to.write_all(bytes).is_err() {
            *writing = false;
        }
    };
    loop {
        let chunk = match reader.fill_buf() {
            Ok([]) => break,
            Ok(chunk) => chunk,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) => {
                markers.read_error = Some(error.to_string());
                break;
            }
        };
        for &byte in chunk {
            if long {
                write(&[byte], &mut writing);
                long = byte != b'\n';
                continue;
            }
            line.push(byte);
            if byte == b'\n' {
                // psql follows each marker query's row with a blank line.
                let marker_blank = after_marker && line == b"\n";
                after_marker = markers.take(&line);
                if !after_marker && !marker_blank {
                    write(&line, &mut writing);
                }
                line.clear();
            } else if line.len() > MARKER_LINE {
                write(&line, &mut writing);
                line.clear();
                long = true;
            }
        }
        let read = chunk.len();
        reader.consume(read);
    }
    if !line.is_empty() && !markers.take(&line) {
        write(&line, &mut writing);
    }
    drop(to.flush());
    markers
}

impl Markers {
    /// Keeps `line` when it is one of berth's markers.
    fn take(&mut self, line: &[u8]) -> bool {
        let text = String::from_utf8_lossy(line);
        let text = text.trim();
        if let Some(pid) = text.strip_prefix(crate::docker::START_MARKER) {
            self.start.get_or_insert_with(|| pid.to_owned());
        } else if let Some(end) = text.strip_prefix(crate::docker::END_MARKER) {
            self.end = Some(end.to_owned());
        } else if let Some(rollback) = text.strip_prefix(crate::docker::ROLLBACK_MARKER) {
            self.rollback = Some(rollback.to_owned());
        } else if let Some(at) = text.find(DUMP_END_MARKER) {
            // Anywhere on the line: a dump's last `\echo -n` leaves its
            // text before berth's token (which cannot be guessed).
            self.dump_end = Some(text[at + DUMP_END_MARKER.len()..].to_owned());
            // Only the database line after the token is berth's.
            self.database = None;
        } else if let Some(database) = text.strip_prefix(DATABASE_MARKER) {
            if self.dump_end.is_some() {
                self.database = Some(database.to_owned());
            }
        } else {
            return false;
        }
        true
    }

    /// Fails unless the client read the whole dump (berth's token after it
    /// came back) and, for MySQL, is still in the service's database.
    fn check_end(&self, end: &DumpEnd, advice: &str) -> Result<()> {
        if let Some(error) = &self.read_error {
            anyhow::bail!(
                "failed to read the database client's output ({error}), so whether the whole \
                 dump ran is unknown; {advice}"
            );
        }
        if self.dump_end.as_deref() != Some(end.token.as_str()) {
            anyhow::bail!(
                "the database client stopped before the end of the dump (it quits, with `\\q` \
                 or `exit`, ends inside an unfinished statement, string, comment or `\\if`, or \
                 sends its output elsewhere with a last `\\o`), so whether all of it ran is \
                 unknown; {advice}"
            );
        }
        if let DumpClient::Mysql { database } = &end.client
            && self.database.as_deref() != Some(database.as_str())
        {
            anyhow::bail!(
                "the dump switched to another database (`USE`, which `mysqldump --databases` \
                 writes), so the rest of it did not go into this service's database `{database}`; \
                 make the dump again naming only the database (`mysqldump <database>`), then \
                 restore it with `--reset`"
            );
        }
        Ok(())
    }

    /// Fails unless the whole dump ran on the connection it began on, left
    /// no transaction open and kept `ON_ERROR_ROLLBACK`.
    fn check(&self, advice: &str) -> Result<()> {
        if let Some(error) = &self.read_error {
            anyhow::bail!(
                "failed to read the database client's output ({error}), so whether the whole \
                 dump ran is unknown; {advice}"
            );
        }
        let (Some(start), Some(end), Some(rollback)) = (&self.start, &self.end, &self.rollback)
        else {
            anyhow::bail!(
                "the database client did not run berth's checks around the dump (a server older \
                 than PostgreSQL 10.7, or 11.0 or 11.1, or berth could not read the client's \
                 output), so whether all of it ran is unknown; {advice}"
            );
        };
        let (end_pid, closed) = end.split_once(' ').unwrap_or((end.as_str(), ""));
        if start != end_pid {
            anyhow::bail!(
                "the dump switched database connection (`\\connect`, which `pg_dump --create` \
                 writes), so the rest of it did not go into this service's database; make the \
                 dump again without `--create`, then restore it with `--reset`"
            );
        }
        if closed != "true" {
            anyhow::bail!(
                "the dump left a transaction open (a `BEGIN` without its `COMMIT`), which the \
                 database client rolls back when it exits; {advice}"
            );
        }
        if !rollback.eq_ignore_ascii_case("on") {
            anyhow::bail!(
                "the dump turned off `ON_ERROR_ROLLBACK`, so one failed statement may have \
                 rolled back a whole transaction around it; {advice}"
            );
        }
        Ok(())
    }
}

/// The most of one stderr line kept: a dump's own `RAISE` can print lines of
/// any length, and an error's text is at its start. An echoed statement
/// (`psql:<stdin>:N: STATEMENT:  …`) is kept whole up to the longest line
/// the line tap classifies, so a long owner change still matches its line.
const KEPT_LINE: usize = 64 * 1024 + 64;

fn read_stderr(stderr: impl Read) -> Stderr {
    let mut kept = Stderr::default();
    let mut reader = BufReader::new(stderr);
    let mut line = Vec::new();
    loop {
        let chunk = match reader.fill_buf() {
            Ok(chunk) => chunk,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) => {
                kept.read_error = Some(error.to_string());
                break;
            }
        };
        if chunk.is_empty() {
            break;
        }
        let (taken, ended) = match chunk.iter().position(|&byte| byte == b'\n') {
            Some(end) => (end + 1, true),
            None => (chunk.len(), false),
        };
        let room = KEPT_LINE.saturating_sub(line.len());
        line.extend(chunk.iter().take(taken.min(room)));
        reader.consume(taken);
        if ended {
            end_line(&mut kept, &line);
            line.clear();
        }
    }
    // The unfinished last line still counts.
    if !line.is_empty() {
        end_line(&mut kept, &line);
    }
    kept
}

fn end_line(kept: &mut Stderr, line: &[u8]) {
    let room = KEPT_STDERR.saturating_sub(kept.start.len());
    kept.start.extend(line.iter().take(room));
    kept.errors
        .observe(String::from_utf8_lossy(line).trim_end());
}

/// psql's errors are read from its output, since it exits 0 whatever
/// failed; `extension_tables` names which denied tables an extension owns.
struct PsqlCheck<'a> {
    service: &'a str,
    extension_tables: &'a dyn Fn(&BTreeSet<String>) -> Result<BTreeSet<String>>,
    statements: &'a StatementLines,
    /// A shared server's tenant, which may not write an extension's schemas.
    shared: bool,
}

/// Ends a restore after streaming: the database's own result wins over a
/// write error. A client that rejects the dump exits and closes its stdin, so
/// berth's next write fails with a broken pipe; reporting that would hide
/// the SQL error that caused it (ledger L117, FI-03).
///
/// # Errors
///
/// Returns the database process's failure, else the streaming error.
pub(super) fn finish_restore(
    service: &ServiceConfig,
    options: RestoreOptions,
    restore: RestoreProcess,
    streamed: Result<()>,
    statements: &std::sync::Mutex<StatementLines>,
) -> Result<()> {
    let lookup =
        |names: &BTreeSet<String>| super::extension_tables::extension_tables(service, names);
    let statements = statements
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let psql = (service.driver == crate::config::Driver::Postgres).then_some(PsqlCheck {
        service: &service.name,
        extension_tables: &lookup,
        statements: &statements,
        shared: service.runtime.shared.is_some(),
    });
    finish_with_advice(restore, streamed, &failure_advice(options), psql)?;
    // A dump tool ends its dump with a marker: without it the stream was
    // cut short (a file and stdin are checked for it before the reset; this
    // is for a dump that changed while it was read).
    if statements.made_by_dump_tool && !statements.complete {
        anyhow::bail!(
            "the dump ends without its dump tool's end-of-dump line, so it was cut short and the \
             database holds a partial restore; restore a whole dump with `--reset`"
        );
    }
    Ok(())
}

/// Wraps the dump so the restore knows which statement each line holds.
pub(super) fn tap(
    service: &ServiceConfig,
    reader: Box<dyn Read>,
) -> (LineTap, std::sync::Arc<std::sync::Mutex<StatementLines>>) {
    let (mut tap, statements) =
        LineTap::new(reader, service.driver == crate::config::Driver::Postgres);
    tap.expect_database(service.database.as_deref().unwrap_or("app"));
    (tap, statements)
}

fn finish_with_advice(
    restore: RestoreProcess,
    streamed: Result<()>,
    advice: &str,
    psql: Option<PsqlCheck<'_>>,
) -> Result<()> {
    let RestoreProcess {
        stdin,
        mut child,
        stderr,
        stdout,
        end,
    } = restore;
    drop(stdin);
    let status = child.wait().context("Failed to wait for restore process")?;
    let stderr = stderr
        .join()
        .map_err(|_| anyhow::anyhow!("the restore client's stderr reader panicked"))?;
    let markers = match stdout {
        Some(stdout) => Some(
            stdout
                .join()
                .map_err(|_| anyhow::anyhow!("the restore client's stdout relay panicked"))?,
        ),
        None => None,
    }; // A dump that broke while it was read is the cause: the client only
    // saw it cut short, so its own error would point at the SQL.
    if let Err(error) = &streamed
        && let Some(read) = super::dump_file::dump_read_error(error)
    {
        return Err(anyhow::anyhow!("{read}"));
    }
    // A dump refused partway (its lines before reached the database).
    if let Err(error) = &streamed
        && let Some(refused) = error.chain().find_map(|cause| {
            cause
                .downcast_ref::<std::io::Error>()
                .and_then(|io| io.get_ref())
                .and_then(|inner| inner.downcast_ref::<super::statement_lines::DumpRefused>())
        })
    {
        return Err(anyhow::anyhow!(
            "the dump {refused}; the database holds a partial restore, so restore a good dump \
             with `--reset`"
        ));
    }
    if let Some(error) = &stderr.read_error {
        anyhow::bail!(
            "failed to read the database client's errors ({error}), so whether every statement \
             worked is unknown; {advice}"
        );
    }
    let output = std::process::Output {
        status,
        stdout: Vec::new(),
        stderr: stderr.start,
    };
    super::super::sql_admin::ensure_sql_command_success(&output, "Database restore failed")
        .map_err(|error| anyhow::anyhow!("{error}; {advice}"))?;
    // The client's own stdout holds berth's markers; a dry run's
    // placeholder client prints none.
    let markers = if crate::docker::is_dry_run() {
        None
    } else {
        let markers = markers.context("the database client's output was not captured")?;
        markers.check_end(&end, advice)?;
        Some(markers)
    };
    if let Some(psql) = psql {
        if let Some(markers) = &markers {
            markers.check(advice)?;
        }
        judge_psql(stderr.errors, &psql, advice)?;
    }
    streamed
}

/// What to do about a dump the database rejected. Its format was checked
/// before the restore began, and a dump that broke while read is reported
/// as such, so the cause is its SQL.
fn failure_advice(options: RestoreOptions) -> String {
    match (options.origin, options.reset) {
        (DumpOrigin::SeedFile, _) => "fix the service's `seed_file` dump".to_owned(),
        (_, true) => {
            "it went into a freshly reset database, so the dump itself needs fixing".to_owned()
        }
        _ => "if the database already had these tables, rerun with `--reset` (it drops the \
              database's current data first)"
            .to_owned(),
    }
}

/// Warns of the errors that leave the data whole and fails on any other.
fn judge_psql(errors: PsqlErrors, psql: &PsqlCheck<'_>, advice: &str) -> Result<()> {
    let denied = errors.denied_names(psql.statements);
    let extension_tables = (psql.extension_tables)(&denied).with_context(|| {
        format!(
            "the restore ran, but {} permission error(s) could not be judged",
            denied.len()
        )
    })?;
    let verdict = errors.verdict(psql.statements, &extension_tables);
    let warn = |message: String| {
        crate::output::event(
            psql.service,
            crate::output::LogLevel::Warn,
            &message,
            crate::output::Persistence::Persistent,
        );
    };
    if verdict.roles > 0 {
        warn(format!(
            "Skipped {} owner, grant, session-authorization or extension-comment statement(s) \
             naming roles this server lacks or this database's role may not act as; the objects \
             load, owned by the role that restores them and without those grants",
            verdict.roles
        ));
    }
    if verdict.extension_tables > 0 {
        warn(format!(
            "Skipped {} write(s) to tables or sequences an extension manages here (empty ones, \
             and TimescaleDB's per-installation metadata): the extension keeps its own for this \
             database",
            verdict.extension_tables
        ));
    }
    if verdict.settings > 0 {
        warn(format!(
            "Skipped {} setting(s) this PostgreSQL does not have (the dump came from a newer \
             one)",
            verdict.settings
        ));
    }
    if verdict.failed_total == 0 {
        return Ok(());
    }
    let shown = verdict
        .failed
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("; ");
    let more = verdict.failed_total.saturating_sub(verdict.failed.len());
    let more = if more > 0 {
        format!(" (and {more} more)")
    } else {
        String::new()
    };
    // TimescaleDB keeps a hypertable's chunks in its own schemas, which a
    // tenant may not write: only a server of the service's own can restore
    // them (as its superuser).
    let hypertables = psql.shared
        && verdict
            .failed
            .iter()
            .any(|failure| failure.message.contains("_timescaledb"));
    // psql before 18.6, 17.11, 16.15, 15.19 and 14.24 (all of 13) runs a
    // refused `COPY … FROM stdin`'s rows as its own input (CVE-2026-6464,
    // https://www.postgresql.org/support/security/CVE-2026-6464/): the dump
    // is fine, the client is not.
    let old_psql = verdict.stray_terminators > 0;
    let mut advice = if hypertables {
        // A dump (a file or stdin) is checked for them before the reset;
        // this is a dump the check could not tell (no pg_dump header).
        format!("the dump {}", super::dump_file::HYPERTABLES)
    } else if old_psql {
        String::new()
    } else {
        advice.to_owned()
    };
    if old_psql {
        if !advice.is_empty() {
            advice.push_str("; also, ");
        }
        advice.push_str(
            "a psql older than 18.6 (17.11, 16.15, 15.19, 14.24) runs the rows of a COPY the \
             server refused as commands: pull a current image of the service (and run `berth \
             lock images` again when the project pins one) and restore again; with a current \
             psql, the dump has a `\\.` outside a COPY's data",
        );
    }
    anyhow::bail!(
        "Database restore failed: {} statement(s) of the dump failed, so the database holds a \
         partial restore: {shown}{more}; {advice}",
        verdict.failed_total
    )
}

#[cfg(test)]
mod tests {
    use super::super::statement_lines::StatementLines;
    use super::{PsqlCheck, RestoreProcess, finish_with_advice};
    use std::collections::BTreeSet;
    use std::io::Write;
    use std::process::{Command, Stdio};

    /// berth's markers as psql prints them after a whole dump on one
    /// connection (`docker/exec/psql_restore.rs`).
    const MARKERS: &str = "printf 'berth-dump-end t0ken\\n berth-backend-start 7\\n\\n berth-backend-end 7 true\\n\\nberth-rollback on\\n'";

    /// berth's line after the dump in these tests.
    fn psql_end() -> super::DumpEnd {
        super::DumpEnd {
            token: "t0ken".to_owned(),
            client: super::DumpClient::Psql,
        }
    }

    /// A client running `script`, then printing berth's markers.
    fn client(script: &str) -> RestoreProcess {
        raw_client(&format!("{script}\n{MARKERS}"))
    }

    fn raw_client(script: &str) -> RestoreProcess {
        let child = Command::new("sh")
            .args(["-c", script])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn client");
        RestoreProcess::new(child, psql_end()).expect("restore process")
    }

    fn no_extension_tables(_: &BTreeSet<String>) -> anyhow::Result<BTreeSet<String>> {
        Ok(BTreeSet::new())
    }

    fn hypertable_is_an_extension_table(
        names: &BTreeSet<String>,
    ) -> anyhow::Result<BTreeSet<String>> {
        Ok(names
            .iter()
            .filter(|name| *name == "hypertable")
            .cloned()
            .collect())
    }

    fn psql<'a>(
        lookup: &'a dyn Fn(&BTreeSet<String>) -> anyhow::Result<BTreeSet<String>>,
        statements: &'a StatementLines,
    ) -> PsqlCheck<'a> {
        PsqlCheck {
            service: "db",
            extension_tables: lookup,
            statements,
            shared: false,
        }
    }

    /// Line 3 an owner statement, line 4 an extension comment, line 5 a
    /// COPY without rows, line 6 a `SET`.
    fn statements() -> StatementLines {
        StatementLines {
            role_statements: BTreeSet::from([3, 4]),
            settings: BTreeSet::from([6]),
            rowless_writes: BTreeSet::from([5]),
            texts: (3..=6)
                .map(|line| {
                    (
                        line,
                        super::super::statement_lines::text_key(&format!("statement {line};")),
                    )
                })
                .collect(),
            ..StatementLines::default()
        }
    }

    #[test]
    fn a_rejected_dump_reports_the_database_error_not_the_broken_pipe() {
        // The "client" reads one line, rejects it and exits; later writes
        // hit a closed pipe.
        let mut restore = client("read line; echo 'ERROR 1064: syntax error near FOO' >&2; exit 1");
        let streamed = (|| -> anyhow::Result<()> {
            for _ in 0..100_000 {
                restore.stdin.write_all(b"FOO BAR;\n")?;
            }
            Ok(())
        })();
        assert!(streamed.is_err(), "the pipe closed");
        let error = finish_with_advice(restore, streamed, "advice", None).expect_err("fails");
        assert!(
            error.to_string().contains("syntax error near FOO"),
            "{error:#}"
        );
    }

    #[test]
    fn a_client_that_prints_more_than_a_pipe_holds_never_blocks_the_restore() {
        // Megabytes of errors before it reads any input, as psql prints one
        // error per row.
        let mut restore = client(
            "i=0; while [ $i -lt 40000 ]; do echo \"psql:<stdin>:$i: ERROR:  duplicate key value violates unique constraint\" >&2; i=$((i+1)); done; cat >/dev/null",
        );
        let streamed = restore
            .stdin
            .write_all(&b"INSERT INTO t VALUES (1);\n".repeat(50_000))
            .map_err(anyhow::Error::from);
        let lookup = no_extension_tables;
        let lines = statements();
        let error = finish_with_advice(restore, streamed, "advice", Some(psql(&lookup, &lines)))
            .expect_err("errors fail it");
        let message = error.to_string();
        assert!(
            message.contains("40000 statement(s) of the dump failed"),
            "{message}"
        );
        assert!(message.contains("line 0: duplicate key value"), "{message}");
        assert!(message.contains("(and 39995 more)"), "{message}");
    }

    #[test]
    fn a_line_of_any_length_costs_bounded_memory_and_keeps_its_error() {
        let restore = client(
            "cat >/dev/null; printf 'psql:<stdin>:7: ERROR:  relation \"x\" already exists ' >&2; \
             head -c 5000000 /dev/zero | tr '\\0' 'a' >&2; echo >&2",
        );
        let lookup = no_extension_tables;
        let lines = statements();
        let error = finish_with_advice(restore, Ok(()), "advice", Some(psql(&lookup, &lines)))
            .expect_err("fails");
        let message = error.to_string();
        assert!(
            message.contains("line 7: relation \"x\" already exists"),
            "{}",
            &message[..200]
        );
        assert!(message.len() < 20_000, "{}", message.len());
    }

    #[test]
    fn a_dump_that_broke_while_read_is_reported_over_the_clients_error() {
        let restore =
            client("cat >/dev/null; echo 'ERROR: syntax error at end of input' >&2; exit 3");
        let broken = std::io::Error::other(super::super::dump_file::DumpReadError {
            cause: "corrupt deflate stream".to_owned(),
            message: "dump.sql.gz is damaged or cut short".to_owned(),
        });
        let error = finish_with_advice(
            restore,
            Err(anyhow::Error::new(broken).context("failed to read dump.sql.gz")),
            "advice",
            None,
        )
        .expect_err("broken dump");
        let message = format!("{error:#}");
        assert!(message.contains("damaged or cut short"), "{message}");
        assert!(!message.contains("syntax error"), "{message}");
    }

    #[test]
    fn psql_errors_fail_the_restore_though_it_exits_zero() {
        let restore = client(
            "cat >/dev/null; echo 'psql:<stdin>:3: ERROR:  relation \"users\" already exists' >&2",
        );
        let lookup = no_extension_tables;
        let lines = statements();
        let error = finish_with_advice(restore, Ok(()), "advice", Some(psql(&lookup, &lines)))
            .expect_err("failed");
        let message = error.to_string();
        assert!(
            message.contains("1 statement(s) of the dump failed"),
            "{message}"
        );
        assert!(
            message.contains("line 3: relation \"users\" already exists"),
            "{message}"
        );
        assert!(message.ends_with("; advice"), "{message}");
    }

    #[test]
    fn errors_that_leave_the_data_whole_leave_the_restore_good() {
        let restore = client(
            "cat >/dev/null; printf '%s\\n' \
             'psql:<stdin>:3: ERROR:  role \"prod_app\" does not exist' \
             'psql:<stdin>:3: STATEMENT:  statement 3;' \
             'psql:<stdin>:4: ERROR:  must be owner of extension timescaledb' \
             'psql:<stdin>:4: STATEMENT:  statement 4;' \
             'psql:<stdin>:5: ERROR:  permission denied for table hypertable' \
             'psql:<stdin>:5: STATEMENT:  statement 5;' \
             'psql:<stdin>:6: ERROR:  unrecognized configuration parameter \"transaction_timeout\"' \
             'psql:<stdin>:6: STATEMENT:  statement 6;' >&2",
        );
        let lookup = hypertable_is_an_extension_table;
        let lines = statements();
        finish_with_advice(restore, Ok(()), "advice", Some(psql(&lookup, &lines)))
            .expect("data whole");
        // A denied table no extension owns is a failure.
        let restore = client(
            "cat >/dev/null; echo 'psql:<stdin>:5: ERROR:  permission denied for table users' >&2",
        );
        let error = finish_with_advice(restore, Ok(()), "advice", Some(psql(&lookup, &lines)))
            .expect_err("users");
        assert!(
            error
                .to_string()
                .contains("permission denied for table users"),
            "{error}"
        );
        // Other clients are judged by their exit code alone.
        let restore = client("cat >/dev/null; echo 'ERROR: anything' >&2");
        finish_with_advice(restore, Ok(()), "advice", None).expect("mysql-like");
    }

    #[test]
    fn a_tenant_that_cannot_restore_hypertables_is_told_to_go_dedicated() {
        let restore = client(
            "cat >/dev/null; echo 'psql:<stdin>:71: ERROR:  permission denied for schema _timescaledb_internal' >&2",
        );
        let lookup = no_extension_tables;
        let lines = statements();
        let check = PsqlCheck {
            shared: true,
            ..psql(&lookup, &lines)
        };
        let error = finish_with_advice(restore, Ok(()), "advice", Some(check)).expect_err("fails");
        assert!(
            error.to_string().contains("`isolation: dedicated`"),
            "{error}"
        );
    }

    #[test]
    fn a_psql_that_runs_refused_copy_rows_is_told_to_update() {
        // psql 13, or an unfixed psql on a dump without pg_dump's
        // `\restrict` fence, after a refused COPY with rows: the rows run as
        // SQL, then its terminator as a command.
        let restore = client(
            "cat >/dev/null; \
             echo 'psql:<stdin>:213: ERROR:  permission denied for table metadata' >&2; \
             echo 'psql:<stdin>:215: error: invalid command \\.' >&2; \
             echo 'psql:<stdin>:216: ERROR:  syntax error at or near \"exported_uuid\"' >&2",
        );
        let lookup = no_extension_tables;
        let lines = statements();
        let error = finish_with_advice(restore, Ok(()), "advice", Some(psql(&lookup, &lines)))
            .expect_err("fails");
        assert!(
            error.to_string().contains("a psql older than 18.6"),
            "{error}"
        );
    }

    #[test]
    fn a_fenced_dumps_stray_terminator_is_told_to_update_even_after_other_failures() {
        // psql 18.5 inside pg_dump's `\restrict` fence words the stray `\.`
        // as a restricted command; five failures before it fill the shown
        // list.
        let restore = client(
            "cat >/dev/null; \
             for n in 1 2 3 4 5; do echo \"psql:<stdin>:$n: ERROR:  relation \\\"t$n\\\" already exists\" >&2; done; \
             echo 'psql:<stdin>:213: ERROR:  permission denied for table metadata' >&2; \
             echo 'psql:<stdin>:215: error: backslash commands are restricted; only \\unrestrict is allowed' >&2",
        );
        let lookup = no_extension_tables;
        let lines = statements();
        let error = finish_with_advice(restore, Ok(()), "advice", Some(psql(&lookup, &lines)))
            .expect_err("fails");
        assert!(
            error.to_string().contains("a psql older than 18.6"),
            "{error}"
        );
    }

    #[test]
    fn a_tenant_with_hypertables_and_an_old_psql_is_told_both() {
        let restore = client(
            "cat >/dev/null; \
             echo 'psql:<stdin>:71: ERROR:  permission denied for schema _timescaledb_internal' >&2; \
             echo 'psql:<stdin>:90: error: invalid command \\.' >&2",
        );
        let lookup = no_extension_tables;
        let lines = statements();
        let check = PsqlCheck {
            shared: true,
            ..psql(&lookup, &lines)
        };
        let error = finish_with_advice(restore, Ok(()), "advice", Some(check)).expect_err("fails");
        let error = error.to_string();
        assert!(error.contains("`isolation: dedicated`"), "{error}");
        assert!(error.contains("a psql older than 18.6"), "{error}");
    }

    #[test]
    fn a_failed_stderr_read_is_no_end_of_errors() {
        // Errors psql prints after a failed read are unknown: the restore
        // cannot be judged good.
        struct Breaks(Option<&'static [u8]>);
        impl std::io::Read for Breaks {
            fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
                match self.0.take() {
                    Some(bytes) => {
                        buf[..bytes.len()].copy_from_slice(bytes);
                        Ok(bytes.len())
                    }
                    None => Err(std::io::Error::other("the pipe broke")),
                }
            }
        }
        let kept = super::read_stderr(Breaks(Some(b"psql:<stdin>:3: NOTICE:  fine\n")));
        assert_eq!(kept.read_error.as_deref(), Some("the pipe broke"));
        // A client that exits 0 with no error it could read still fails.
        let child = Command::new("sh")
            .args(["-c", "cat >/dev/null"])
            .stdin(Stdio::piped())
            .spawn()
            .expect("spawn client");
        let mut child = child;
        let restore = RestoreProcess {
            stdin: child.stdin.take().expect("stdin"),
            child,
            stderr: std::thread::spawn(|| super::read_stderr(Breaks(None))),
            stdout: None,
            end: psql_end(),
        };
        let lookup = no_extension_tables;
        let lines = statements();
        let error = finish_with_advice(restore, Ok(()), "advice", Some(psql(&lookup, &lines)))
            .expect_err("unknown errors fail");
        assert!(
            error
                .to_string()
                .contains("failed to read the database client's errors"),
            "{error}"
        );
    }

    #[test]
    fn psqls_stdout_is_relayed_without_berths_markers() {
        let mut relayed = Vec::new();
        let output = " berth-backend-start 7\n\nSET\n\nCREATE TABLE\n berth-backend-end 7 true\n\nberth-rollback on\n";
        let markers = super::relay_stdout(output.as_bytes(), &mut relayed);
        // The markers and the blank line after each go; the dump's own
        // blank lines stay.
        assert_eq!(String::from_utf8_lossy(&relayed), "SET\n\nCREATE TABLE\n");
        assert_eq!(
            (
                markers.start.as_deref(),
                markers.end.as_deref(),
                markers.rollback.as_deref()
            ),
            (Some("7"), Some("7 true"), Some("on"))
        );
        // The first start and the last end count: the dump runs between them.
        let markers = super::relay_stdout(
            " berth-backend-start 7\n berth-backend-start 9\n berth-backend-end 9 true\n berth-backend-end 7 true\nberth-rollback on\n"
                .as_bytes(),
            std::io::sink(),
        );
        assert_eq!(
            (markers.start.as_deref(), markers.end.as_deref()),
            (Some("7"), Some("7 true"))
        );
        // A long line is the dump's own, passed on whole.
        let long = format!("{}\n", "x".repeat(10_000));
        let mut relayed = Vec::new();
        super::relay_stdout(long.as_bytes(), &mut relayed);
        assert_eq!(relayed, long.as_bytes());
    }

    #[test]
    fn a_restore_fails_unless_berths_markers_show_the_whole_dump_ran_on_one_connection() {
        let lookup = no_extension_tables;
        let lines = statements();
        for (script, expected) in [
            // `\c` (even quietly): the end is another server process.
            (
                "printf 'berth-dump-end t0ken\\n berth-backend-start 7\\n berth-backend-end 8 true\\nberth-rollback on\\n'",
                "switched database connection",
            ),
            (
                "printf 'berth-dump-end t0ken\\n berth-backend-start 7\\n berth-backend-end 7 false\\nberth-rollback on\\n'",
                "left a transaction open",
            ),
            (
                "printf 'berth-dump-end t0ken\\n berth-backend-start 7\\n berth-backend-end 7 true\\nberth-rollback off\\n'",
                "turned off `ON_ERROR_ROLLBACK`",
            ),
            // A server older than PostgreSQL 10 (the end query fails), or
            // 10.7 / 11.2 (the start query returns no row).
            (
                "printf 'berth-dump-end t0ken\\n berth-backend-start 7\\n'",
                "did not run berth's check",
            ),
            (
                "printf 'berth-dump-end t0ken\\n berth-backend-end 7 true\\nberth-rollback on\\n'",
                "did not run berth's check",
            ),
            // `\q` ends psql's reading of the dump, not its commands after
            // it: only berth's token shows the dump was read whole.
            (
                "printf ' berth-backend-start 7\\n berth-backend-end 7 true\\nberth-rollback on\\n'",
                "stopped before the end of the dump",
            ),
            // A token, but not berth's (the dump printed one itself).
            (
                "printf 'berth-dump-end forged\\n berth-backend-start 7\\n berth-backend-end 7 true\\nberth-rollback on\\n'",
                "stopped before the end of the dump",
            ),
        ] {
            let restore = raw_client(&format!("cat >/dev/null; {script}"));
            let error = finish_with_advice(restore, Ok(()), "advice", Some(psql(&lookup, &lines)))
                .expect_err(expected);
            assert!(error.to_string().contains(expected), "{error}");
        }
        // A client whose output was not captured is no proof.
        let child = Command::new("sh")
            .args(["-c", "cat >/dev/null"])
            .stdin(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn client");
        let restore = RestoreProcess::new(child, psql_end()).expect("restore process");
        let error = finish_with_advice(restore, Ok(()), "advice", Some(psql(&lookup, &lines)))
            .expect_err("no output");
        assert!(error.to_string().contains("not captured"), "{error}");
    }

    #[test]
    fn a_failed_stdout_read_is_no_proof_the_dump_ran() {
        struct Breaks;
        impl std::io::Read for Breaks {
            fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
                Err(std::io::Error::other("the pipe broke"))
            }
        }
        let markers = super::relay_stdout(Breaks, std::io::sink());
        let error = markers.check("advice").expect_err("unknown");
        assert!(
            error
                .to_string()
                .contains("failed to read the database client's output")
        );
    }

    #[test]
    fn a_streaming_error_stands_when_the_database_succeeded() {
        let restore = client("cat >/dev/null");
        let error = finish_with_advice(
            restore,
            Err(anyhow::anyhow!("Failed to read from SQL dump file")),
            "advice",
            None,
        )
        .expect_err("read error");
        assert!(error.to_string().contains("Failed to read"), "{error:#}");
    }

    #[test]
    fn a_failed_client_reports_its_non_utf8_stderr() {
        let restore = client("printf '\\377' >&2; exit 1");
        let rendered = finish_with_advice(restore, Ok(()), "advice", None)
            .expect_err("expected restore failure")
            .to_string();
        assert!(rendered.starts_with("Database restore failed:"));
        assert_ne!(rendered.trim_end(), "Database restore failed:");
    }

    #[test]
    fn the_advice_follows_the_dump_and_reset() {
        use super::super::{DumpOrigin, RestoreOptions};
        let advice = |origin, reset| {
            super::failure_advice(RestoreOptions {
                reset,
                gzip: false,
                origin,
            })
        };
        assert!(advice(DumpOrigin::FileFlag, false).contains("rerun with `--reset`"));
        assert!(!advice(DumpOrigin::FileFlag, true).contains("--reset`"));
        assert!(advice(DumpOrigin::SeedFile, false).contains("`seed_file`"));
    }

    #[test]
    fn a_mysql_restore_must_end_in_the_services_database() {
        let end = || super::DumpEnd {
            token: "t0ken".to_owned(),
            client: super::DumpClient::Mysql {
                database: "app".to_owned(),
            },
        };
        let mysql = |script: &str| {
            let child = Command::new("sh")
                .args(["-c", &format!("cat >/dev/null; {script}")])
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .expect("spawn client");
            let restore = RestoreProcess::new(child, end()).expect("restore process");
            finish_with_advice(restore, Ok(()), "advice", None)
        };
        mysql("printf 'berth-dump-end t0ken\\nberth-db app\\n'").expect("whole");
        let error = mysql("printf 'berth\\nberth-dump-end t0ken\\nberth\\nberth-db prod\\n'")
            .expect_err("switched");
        assert!(
            error.to_string().contains("switched to another database"),
            "{error}"
        );
        let error = mysql("printf 'berth\\nberth-db app\\n'").expect_err("quit early");
        assert!(
            error.to_string().contains("stopped before the end"),
            "{error}"
        );
        // A database line before the token is the dump's, not berth's.
        let error =
            mysql("printf 'berth-db app\\nberth-dump-end t0ken\\n'").expect_err("no database");
        assert!(
            error.to_string().contains("switched to another database"),
            "{error}"
        );
        // What berth sends: the token, then the database it is in.
        let trailer = end().trailer();
        assert!(trailer.contains("SELECT 'berth-dump-end t0ken' AS berth;"));
        assert!(trailer.contains("DATABASE()"));
        assert_eq!(psql_end().trailer(), "\n\\echo berth-dump-end t0ken\n");
    }

    #[test]
    fn berths_token_is_found_after_a_dumps_open_echo() {
        // A dump's last `\echo -n x` leaves `x` before the token's line.
        let markers = super::relay_stdout(b"xberth-dump-end t0ken\n".as_slice(), std::io::sink());
        markers
            .check_end(&psql_end(), "advice")
            .expect("the token came back");
    }
}
