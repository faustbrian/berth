//! What each line of a dump holds, for judging the errors psql names by
//! line and for checking a dump before anything is dropped.

use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::sync::{Arc, Mutex};

mod lexer;

/// How much of a line is kept to classify it: pg_dump writes owner, grant,
/// `SET`, `setval` and `COPY` statements on one line each, short but for a
/// wide table's `COPY` header (one column name after another).
const KEPT_LINE: usize = 64 * 1024;

/// The lines (1-based, as psql counts them, `COPY` data included) that hold
/// statements whose errors leave the data whole, and what the dump says
/// about itself.
#[derive(Debug, Default)]
pub(super) struct StatementLines {
    /// `ALTER … OWNER TO`, `GRANT`, `REVOKE`, `ALTER DEFAULT PRIVILEGES`,
    /// `SET SESSION AUTHORIZATION '…'`, `COMMENT ON EXTENSION`.
    pub(super) role_statements: BTreeSet<u64>,
    /// Top-level `SET name = …;` (a setting an older server may lack).
    pub(super) settings: BTreeSet<u64>,
    /// Writes that carry no rows: a `COPY` whose data ends at once (`\.`),
    /// and `SELECT pg_catalog.setval(…)`.
    pub(super) rowless_writes: BTreeSet<u64>,
    /// `COPY`s into TimescaleDB's `metadata`, rows or not (see
    /// [`TIMESCALEDB_METADATA`]).
    pub(super) bookkeeping_copies: BTreeSet<u64>,
    /// The table each `COPY` line loads, as the dump names it (with its
    /// schema): a permission error names only the bare table.
    pub(super) copy_tables: BTreeMap<u64, String>,
    /// The text of each line above: an error is judged by its line only
    /// when the statement psql echoes for it (`--echo-errors`) is that
    /// line, so a line this tap misreads (psql's lexer is followed, not run)
    /// never excuses another statement's error.
    pub(super) texts: BTreeMap<u64, TextKey>,
    /// The `\.` line ending each `COPY` line's data: psql 15+ echoes a
    /// refused `COPY` there, once it skipped the data (psql 13 and 14 echo
    /// it on its own line, before skipping).
    pub(super) copy_ends: BTreeMap<u64, u64>,
    /// The dump names its maker (pg_dump or mysqldump) in its header.
    pub(super) made_by_dump_tool: bool,
    /// Its maker's end-of-dump line was seen (since its last header).
    pub(super) complete: bool,
    /// It holds TimescaleDB hypertables: chunk tables, or hypertable rows.
    pub(super) hypertables: bool,
}

/// TimescaleDB's per-installation bookkeeping (`install_timestamp`,
/// `timescaledb_version`, `exported_uuid`): pg_dump writes its rows into
/// every dump of a database with the extension (timescaledb
/// `sql/pre_install/tables.sql` marks it a dump config table), and the
/// target database's own extension keeps rows of its own. A tenant may not
/// write it; skipping the source's rows loses no data of the dump.
pub(super) const TIMESCALEDB_METADATA: &str = "_timescaledb_catalog.metadata";

/// A line's text as compared with psql's echo of a statement: its length
/// and hash, so a dump of millions of grants keeps no copy of their text.
pub(super) type TextKey = (usize, u64);

pub(super) fn text_key(text: &str) -> TextKey {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut hasher);
    (text.len(), hasher.finish())
}

/// Why a dump is refused while it is read: before anything is changed for
/// a file (its check reads it whole), and before the line reaches the
/// client for stdin.
#[derive(Debug)]
pub(super) struct DumpRefused(pub(super) String);

impl std::fmt::Display for DumpRefused {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for DumpRefused {}

/// Reads through, noting [`StatementLines`].
pub(super) struct LineTap {
    inner: Box<dyn Read>,
    lines: Arc<Mutex<StatementLines>>,
    number: u64,
    current: Vec<u8>,
    /// The `COPY` on the line before and its table, whose first data line
    /// decides whether it carries rows.
    open_copy: Option<(u64, Option<String>)>,
    /// Inside the data of the `COPY` on this line, until `\.`.
    in_copy_data: Option<u64>,
    /// The current line is `COPY` data, which psql does not lex as SQL.
    data_line: bool,
    /// Where psql ends statements, fed every byte of every SQL line of a
    /// PostgreSQL dump.
    lexer: lexer::Lexer,
    /// The current line ran past [`KEPT_LINE`]: too long to classify.
    truncated: bool,
    /// The last bytes of a line past [`KEPT_LINE`]: a wide table's `COPY`
    /// header still ends `FROM stdin;`.
    tail: Vec<u8>,
    /// The dump goes to psql, whose reading of it the tap follows; another
    /// client's dump (MySQL's, MariaDB's) is checked only for its end, a
    /// switch at a line's start, and NUL bytes.
    postgres: bool,
    /// The dump is refused at the line just read.
    refused: Option<DumpRefused>,
    /// The service's database: a MySQL dump may `USE` it, and no other.
    database: Option<String>,
}

/// How much of a line's end is kept past [`KEPT_LINE`].
const KEPT_TAIL: usize = 16;

impl LineTap {
    pub(super) fn new(inner: Box<dyn Read>, postgres: bool) -> (Self, Arc<Mutex<StatementLines>>) {
        let lines = Arc::new(Mutex::new(StatementLines::default()));
        let tap = Self {
            inner,
            lines: Arc::clone(&lines),
            number: 1,
            current: Vec::new(),
            open_copy: None,
            in_copy_data: None,
            data_line: false,
            lexer: lexer::Lexer::new(),
            truncated: false,
            tail: Vec::new(),
            postgres,
            refused: None,
            database: None,
        };
        (tap, lines)
    }

    /// Names the service's database, which a MySQL dump may `USE`.
    pub(super) fn expect_database(&mut self, database: &str) {
        self.database = Some(database.to_owned());
    }

    fn end_line(&mut self) {
        let text = String::from_utf8_lossy(&self.current).into_owned();
        let line = text.trim_end_matches('\r');
        let mut lines = self
            .lines
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if !self.postgres {
            dump_facts(line, &mut lines);
            if switches_mysql_database(line, self.database.as_deref()) {
                self.refused = Some(DumpRefused(format!(
                    "switches to another database on line {} (`USE`, which `mysqldump \
                     --databases` writes), so it would not restore into this service's \
                     database: make the dump again naming only the database \
                     (`mysqldump <database>`)",
                    self.number
                )));
            }
        } else if let Some((copy, table)) = self.open_copy.take() {
            if line != "\\." {
                self.in_copy_data = Some(copy);
                if table.as_deref() == Some("_timescaledb_catalog.hypertable") {
                    lines.hypertables = true;
                }
            } else {
                lines.copy_ends.insert(copy, self.number);
                if table.is_some() {
                    lines.rowless_writes.insert(copy);
                }
            }
        } else if let Some(copy) = self.in_copy_data {
            if line == "\\." {
                lines.copy_ends.insert(copy, self.number);
                self.in_copy_data = None;
            }
        } else {
            let shape = self.lexer.end_line();
            // A literal's or a body's line is not the dump's own; a dump this
            // lexer lost track of is read line by line, as its maker writes
            // these.
            if shape.began_clear() || self.lexer.lost() {
                dump_facts(line, &mut lines);
            }
            let end = if self.truncated {
                String::from_utf8_lossy(&self.tail).into_owned()
            } else {
                line.to_owned()
            };
            // psql names the line a statement ends on, not the one it starts
            // on: only a line holding exactly one statement, begun and ended
            // there (pg_dump writes each on its own), says which statement an
            // error is on, and only a `COPY` statement starts data.
            // berth runs psql with `ON_ERROR_ROLLBACK` on, so one skipped
            // grant cannot roll back the transaction around it; a dump that
            // turns it off for a while could lose one silently. Looked for
            // in literals too: the lexer follows psql's reading, it does not
            // run it.
            if sets_on_error_rollback(line) {
                self.refused = Some(DumpRefused(format!(
                    "sets psql's `ON_ERROR_ROLLBACK` on line {}, which berth's restore keeps on \
                     (so a skipped grant never rolls back the transaction around it): remove \
                     that line",
                    self.number
                )));
            }
            if shape.connects() {
                self.refused = Some(DumpRefused(format!(
                    "switches to another database on line {} (`\\connect`, which `pg_dump \
                     --create` writes), so it would not restore into this service's database: \
                     make the dump again without `--create`",
                    self.number
                )));
            }
            if shape.copy_from_stdin() {
                // psql reads the lines after any `COPY … FROM stdin` as data;
                // only pg_dump's own line says which table, and only one
                // holding the statement alone says where the data starts.
                let table = shape
                    .single_statement()
                    .then(|| copy_table(line, end.trim_end_matches('\r')))
                    .flatten();
                if let Some(table) = table {
                    if table == TIMESCALEDB_METADATA {
                        lines.bookkeeping_copies.insert(self.number);
                    }
                    lines.copy_tables.insert(self.number, table.to_owned());
                    lines.texts.insert(self.number, text_key(line));
                } else if !shape.single_statement() {
                    self.lexer.lose_track();
                }
                self.open_copy = Some((self.number, table.map(ToOwned::to_owned)));
            } else if shape.single_statement() && !self.truncated && !self.lexer.lost() {
                classify(line, self.number, shape.owner_change(), &mut lines);
            }
        }
        drop(lines);
        self.data_line = self.open_copy.is_some() || self.in_copy_data.is_some();
        self.current.clear();
        self.tail.clear();
        self.truncated = false;
        self.number += 1;
    }
}

/// The table a `COPY <table> (…) FROM stdin;` line loads, from its start
/// and its `end`.
fn copy_table<'a>(line: &'a str, end: &str) -> Option<&'a str> {
    if !end.ends_with("FROM stdin;") {
        return None;
    }
    line.strip_prefix("COPY ")?.split([' ', '(']).next()
}

/// Whether a psql command on `line` may set `ON_ERROR_ROLLBACK`: a `\set`
/// or `\unset` whose first argument is that name, or is not a plain name
/// (`:var` and backtick arguments expand to anything).
fn sets_on_error_rollback(line: &str) -> bool {
    line.match_indices('\\').any(|(at, _)| {
        let mut words = line[at + 1..].split_whitespace();
        matches!(words.next(), Some("set" | "unset"))
            && words.next().is_some_and(|name| {
                name == "ON_ERROR_ROLLBACK"
                    || !name
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
            })
    })
}

/// A mysql client line that changes the default database to another than
/// `database`: one that starts with `USE db;`, or the client's `\u db`,
/// `connect db` or `\r db`, as `mysqldump --databases` writes them. The
/// target must be exactly the service's database (a plain or
/// backtick-quoted name, then only `;`, whitespace or a comment); anything
/// else, a name berth cannot read or none on the line included, is
/// refused. berth's check after the dump (`DATABASE()`) catches a switch
/// that lasts to the end. A switch later on a line, or one a dump reverses
/// before its end, is not looked for: following the client's reading of
/// every line is not attempted (a dump can write into other databases by
/// qualified names anyway; only the database's privileges stop that).
fn switches_mysql_database(line: &str, database: Option<&str>) -> bool {
    let mut segment = line.trim_start();
    // A versioned comment's body runs on servers of that version or later.
    if let Some(body) = segment.strip_prefix("/*!") {
        segment = body
            .trim_start_matches(|c: char| c.is_ascii_digit())
            .trim_start();
    }
    let lower = segment.to_ascii_lowercase();
    let ends_word = |after: &str| after.is_empty() || after.starts_with([' ', '\t', '`', '/']);
    // `USE` and `connect` in any case; `\u` and `\r` as typed (`\R` sets
    // the prompt).
    let rest = ["use", "connect"]
        .iter()
        .find_map(|word| {
            lower
                .strip_prefix(word)
                .filter(|after| ends_word(after))
                .map(|_| segment.get(word.len()..).unwrap_or_default())
        })
        .or_else(|| {
            ["\\u", "\\r"].iter().find_map(|command| {
                segment
                    .strip_prefix(command)
                    .filter(|after| ends_word(after))
            })
        });
    let Some(rest) = rest else {
        return false;
    };
    database.is_none_or(|database| mysql_target(rest.trim_start()).as_deref() != Some(database))
}

/// The database a `USE` line's rest names: `` `name` `` (with ```` `` ```` for a
/// backtick) or a bare word, then nothing but `;`, whitespace or a comment.
fn mysql_target(rest: &str) -> Option<String> {
    let (name, after) = if let Some(quoted) = rest.strip_prefix('`') {
        let mut name = String::new();
        let mut chars = quoted.char_indices();
        let mut end = None;
        while let Some((at, character)) = chars.next() {
            if character == '`' {
                if quoted[at + 1..].starts_with('`') {
                    name.push('`');
                    chars.next();
                } else {
                    end = Some(at + 1);
                    break;
                }
            } else {
                name.push(character);
            }
        }
        (name, quoted.get(end?..)?)
    } else {
        let end = rest
            .find(|character: char| character.is_whitespace() || character == ';')
            .unwrap_or(rest.len());
        (rest[..end].to_owned(), &rest[end..])
    };
    let after = after.trim_start();
    let after = after.strip_prefix(';').unwrap_or(after).trim_start();
    (!name.is_empty()
        && (after.is_empty()
            || after.starts_with("--")
            || after.starts_with('#')
            || after.starts_with("/*")))
    .then_some(name)
}

/// What the dump holds, from a line of its own (pg_dump writes a `CREATE
/// TABLE` over several, and its header and end as comments).
fn dump_facts(line: &str, lines: &mut StatementLines) {
    if line.starts_with("CREATE TABLE _timescaledb_internal.") {
        lines.hypertables = true;
    } else if line == "-- PostgreSQL database dump"
        || line.starts_with("-- MySQL dump ")
        || line.starts_with("-- MariaDB dump ")
    {
        // `cat a.sql b.sql`: the first dump's end does not end the second.
        lines.made_by_dump_tool = true;
        lines.complete = false;
    } else if line == "-- PostgreSQL database dump complete"
        || line.starts_with("-- Dump completed")
    {
        lines.complete = true;
    }
}

/// Notes a line holding one whole statement.
fn classify(line: &str, number: u64, owner_change: bool, lines: &mut StatementLines) {
    // `ALTER DEFAULT PRIVILEGES FOR ROLE … GRANT …` only sets the grants of
    // objects a role makes later: no data hangs on it. pg_dump
    // `--use-set-session-authorization` names each object's owner with `SET
    // SESSION AUTHORIZATION 'role';`: without that role the session keeps
    // its previous authorization, which makes the objects all the same (and
    // a statement that role may not run fails on its own).
    let noted = if (owner_change && is_owner_change(line))
        || line.starts_with("SET SESSION AUTHORIZATION '")
        || line.starts_with("ALTER DEFAULT PRIVILEGES ")
        || line.starts_with("GRANT ")
        || line.starts_with("REVOKE ")
        || line.starts_with("COMMENT ON EXTENSION ")
    {
        lines.role_statements.insert(number)
    } else if line.starts_with("SET ")
        || pg_dump_call(line, "set_config", [Arg::Text, Arg::Text, Arg::Flag])
    {
        lines.settings.insert(number)
    } else if pg_dump_call(line, "setval", [Arg::Text, Arg::Number, Arg::Flag]) {
        lines.rowless_writes.insert(number)
    } else {
        false
    };
    if noted {
        lines.texts.insert(number, text_key(line));
    }
}

/// The kinds of object pg_dump changes the owner of
/// (`pg_backup_archiver.c`'s `_getObjectDescription`), longest first so
/// `MATERIALIZED VIEW` is not read as `VIEW` of an object `MATERIALIZED`.
const OWNED_KINDS: [&str; 29] = [
    "TEXT SEARCH CONFIGURATION ",
    "TEXT SEARCH DICTIONARY ",
    "FOREIGN DATA WRAPPER ",
    "PROCEDURAL LANGUAGE ",
    "MATERIALIZED VIEW ",
    "OPERATOR FAMILY ",
    "OPERATOR CLASS ",
    "EVENT TRIGGER ",
    "FOREIGN TABLE ",
    "LARGE OBJECT ",
    "SUBSCRIPTION ",
    "PUBLICATION ",
    "TABLESPACE ",
    "CONVERSION ",
    "STATISTICS ",
    "AGGREGATE ",
    "COLLATION ",
    "PROCEDURE ",
    "SEQUENCE ",
    "FUNCTION ",
    "DATABASE ",
    "LANGUAGE ",
    "OPERATOR ",
    "SCHEMA ",
    "DOMAIN ",
    "SERVER ",
    "TABLE ",
    "TYPE ",
    "VIEW ",
];

/// Whether `line` is `ALTER <kind> <name> OWNER TO <role>;`, as pg_dump
/// writes it: an owner change and nothing else (`RENAME COLUMN owner TO`,
/// a role's or database's `SET x.owner TO` are not one).
fn is_owner_change(line: &str) -> bool {
    let Some(rest) = line.strip_prefix("ALTER ").and_then(|rest| {
        OWNED_KINDS
            .iter()
            .find_map(|kind| rest.strip_prefix(kind).map(|rest| (*kind, rest)))
    }) else {
        return false;
    };
    let (kind, rest) = rest;
    let Some((name_length, after)) = name_end(rest) else {
        return false;
    };
    if name_length == 0 {
        return false;
    }
    // An operator class or family names its index method too.
    let after = if kind.starts_with("OPERATOR ") && kind != "OPERATOR " {
        let Some(method) = after.strip_prefix(" USING ") else {
            return false;
        };
        match method.split_once(' ') {
            Some((word, rest)) if is_plain_name(word) => rest,
            _ => return false,
        }
    } else {
        after.strip_prefix(' ').unwrap_or("")
    };
    after
        .strip_prefix("OWNER TO ")
        .and_then(|role| role.strip_suffix(';'))
        .is_some_and(|role| is_plain_name(role) || is_quoted_name(role))
}

/// The length of the object name `text` starts with: up to the first space
/// outside `"…"` and parentheses (a function's argument list), and the rest.
fn name_end(text: &str) -> Option<(usize, &str)> {
    let mut quoted = false;
    let mut depth = 0_usize;
    for (at, byte) in text.bytes().enumerate() {
        match byte {
            b'"' => quoted = !quoted,
            b'(' if !quoted => depth += 1,
            b')' if !quoted => depth = depth.checked_sub(1)?,
            b' ' if !quoted && depth == 0 => return Some((at, text.get(at..)?)),
            b'\'' | b';' if !quoted => return None,
            _ => {}
        }
    }
    None
}

fn is_plain_name(word: &str) -> bool {
    !word.is_empty()
        && word
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'$')
}

/// `"…"`, with `""` for a quote inside.
fn is_quoted_name(word: &str) -> bool {
    word.len() >= 2
        && word.starts_with('"')
        && word.ends_with('"')
        && word
            .get(1..word.len() - 1)
            .is_some_and(|inner| inner.replace("\"\"", "").bytes().all(|byte| byte != b'"'))
}

/// An argument of a call pg_dump writes.
#[derive(Clone, Copy)]
enum Arg {
    /// `'…'`, with `''` for a quote.
    Text,
    /// Digits, maybe negative.
    Number,
    /// `true` or `false`.
    Flag,
}

/// Whether `line` is exactly `SELECT pg_catalog.<function>(<args>);`, as
/// pg_dump writes it: a statement that calls nothing else and writes no
/// other row.
fn pg_dump_call(line: &str, function: &str, args: [Arg; 3]) -> bool {
    let Some(mut rest) = line
        .strip_prefix("SELECT pg_catalog.")
        .and_then(|rest| rest.strip_prefix(function))
        .and_then(|rest| rest.strip_prefix('('))
    else {
        return false;
    };
    for (index, arg) in args.into_iter().enumerate() {
        if index > 0 {
            let Some(after) = rest.strip_prefix(", ") else {
                return false;
            };
            rest = after;
        }
        let taken = match arg {
            Arg::Text => text_length(rest),
            Arg::Number => {
                let sign = usize::from(rest.starts_with('-'));
                let digits = rest
                    .get(sign..)
                    .unwrap_or_default()
                    .bytes()
                    .take_while(u8::is_ascii_digit)
                    .count();
                (digits > 0).then_some(sign + digits)
            }
            Arg::Flag => ["true", "false"]
                .into_iter()
                .find(|flag| rest.starts_with(flag))
                .map(str::len),
        };
        let Some(after) = taken.and_then(|taken| rest.get(taken..)) else {
            return false;
        };
        rest = after;
    }
    rest == ");"
}

/// The length of the `'…'` literal `text` starts with.
fn text_length(text: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    if bytes.first() != Some(&b'\'') {
        return None;
    }
    let mut index = 1;
    while let Some(&byte) = bytes.get(index) {
        if byte == b'\'' {
            if bytes.get(index + 1) == Some(&b'\'') {
                index += 2;
                continue;
            }
            return Some(index + 1);
        }
        index += 1;
    }
    None
}

impl Read for LineTap {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let read = self.inner.read(buf)?;
        // A last line without a newline is still a line.
        if read == 0 && !self.current.is_empty() {
            self.end_line();
            if let Some(refused) = self.refused.take() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    refused,
                ));
            }
        }
        for &byte in buf.get(..read).unwrap_or_default() {
            // psql ends a line at a NUL (`gets_fromFile` copies it as a C
            // string) and drops the rest with its newline, so its line
            // numbers would no longer be the dump's.
            // The mysql client reads text too (mysqldump writes a NUL as
            // `\0`).
            if byte == 0 {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    DumpRefused(format!(
                        "holds a NUL byte on line {}, which the client does not read as SQL \
                         text (psql merges it with the next line): make the dump again",
                        self.number
                    )),
                ));
            }
            if byte == b'\n' {
                self.end_line();
                if let Some(refused) = self.refused.take() {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        refused,
                    ));
                }
            } else {
                if self.postgres && !self.data_line {
                    self.lexer.feed(byte);
                }
                if self.current.len() < KEPT_LINE {
                    self.current.push(byte);
                } else {
                    self.truncated = true;
                    if self.tail.len() == KEPT_TAIL {
                        self.tail.remove(0);
                    }
                    self.tail.push(byte);
                }
            }
        }
        Ok(read)
    }
}

#[cfg(test)]
mod tests {
    use super::LineTap;
    use std::io::Read;

    #[test]
    fn statements_are_noted_by_the_line_psql_names() {
        let dump = "\
--
-- PostgreSQL database dump
--
SET statement_timeout = 0;
ALTER TABLE public.users OWNER TO prod_app;
GRANT SELECT ON TABLE public.users TO reporting;
COMMENT ON EXTENSION timescaledb IS 'x';
SET SESSION AUTHORIZATION 'prod_app';
COPY _timescaledb_catalog.metadata (key, value) FROM stdin;
\\.
COPY public.users (id) FROM stdin;
GRANT looks like a statement but is data
\\.
SELECT pg_catalog.setval('public.users_id_seq', 1, false);
CREATE POLICY p ON public.users TO app_role;
-- PostgreSQL database dump complete
";
        let (mut tap, lines) = LineTap::new(Box::new(std::io::Cursor::new(dump)), true);
        let mut through = String::new();
        tap.read_to_string(&mut through).expect("read");
        assert_eq!(through, dump, "the dump passes through whole");
        let lines = lines.lock().expect("lines");
        assert_eq!(
            lines.role_statements.iter().copied().collect::<Vec<_>>(),
            [5, 6, 7, 8]
        );
        assert_eq!(lines.settings.iter().copied().collect::<Vec<_>>(), [4]);
        assert_eq!(
            lines.rowless_writes.iter().copied().collect::<Vec<_>>(),
            [9, 14]
        );
        assert!(lines.made_by_dump_tool && lines.complete && !lines.hypertables);
        assert_eq!(
            lines.copy_tables.get(&9).map(String::as_str),
            Some("_timescaledb_catalog.metadata")
        );
        assert_eq!(
            lines.bookkeeping_copies.iter().copied().collect::<Vec<_>>(),
            [9]
        );
    }

    #[test]
    fn hypertables_and_a_missing_end_are_noticed() {
        let dump = "\
-- PostgreSQL database dump
CREATE TABLE _timescaledb_internal._hyper_1_1_chunk (t timestamptz);
COPY _timescaledb_catalog.hypertable (id) FROM stdin;
1
\\.
";
        let (mut tap, lines) = LineTap::new(Box::new(std::io::Cursor::new(dump)), true);
        std::io::copy(&mut tap, &mut std::io::sink()).expect("read");
        let lines = lines.lock().expect("lines");
        assert!(lines.hypertables);
        assert!(lines.made_by_dump_tool && !lines.complete);
    }

    #[test]
    fn a_line_that_ends_an_earlier_statement_is_not_one() {
        // psql reports each error on the line its statement ends: the
        // `SET`/`GRANT` that ends an UPDATE, a function or a schema must not
        // excuse its failure.
        let dump = "UPDATE settings\n\
                    SET value = current_setting('app.tenant');\n\
                    CREATE FUNCTION f() RETURNS int LANGUAGE sql AS 'SELECT 1'\n\
                    SET transaction_timeout = 0;\n\
                    CREATE SCHEMA s AUTHORIZATION missing\n\
                    GRANT SELECT ON TABLES TO app;\n\
                    \n\
                    -- a comment between statements\n\
                    SET statement_timeout = 0;\n\
                    GRANT SELECT ON t TO app;\n";
        let (mut tap, lines) = LineTap::new(Box::new(std::io::Cursor::new(dump)), true);
        std::io::copy(&mut tap, &mut std::io::sink()).expect("read");
        let lines = lines.lock().expect("lines");
        assert_eq!(lines.settings.iter().copied().collect::<Vec<_>>(), [9]);
        assert_eq!(
            lines.role_statements.iter().copied().collect::<Vec<_>>(),
            [10]
        );
    }

    #[test]
    fn a_wide_tables_copy_and_a_long_line_keep_the_statements_after_them() {
        let columns = (0..600)
            .map(|n| format!("column_{n}"))
            .collect::<Vec<_>>()
            .join(", ");
        let long_value = "x".repeat(70 * 1024);
        let dump = format!(
            "COPY public.wide ({columns}) FROM stdin;\n\\.\n\
             INSERT INTO t VALUES ('{long_value}');\n\
             GRANT SELECT ON t TO app;\n\
             ALTER DEFAULT PRIVILEGES FOR ROLE prod_app IN SCHEMA public GRANT SELECT ON TABLES TO app;\n"
        );
        let (mut tap, lines) = LineTap::new(Box::new(std::io::Cursor::new(dump)), true);
        std::io::copy(&mut tap, &mut std::io::sink()).expect("read");
        let lines = lines.lock().expect("lines");
        assert_eq!(
            lines.copy_tables.get(&1).map(String::as_str),
            Some("public.wide")
        );
        assert_eq!(
            lines.rowless_writes.iter().copied().collect::<Vec<_>>(),
            [1]
        );
        // The INSERT past the kept length still ends at its `;`.
        assert_eq!(
            lines.role_statements.iter().copied().collect::<Vec<_>>(),
            [4, 5]
        );
    }

    #[test]
    fn a_psql_command_line_does_not_open_a_statement() {
        // pg_dump 17.6+ fences its dump with `\restrict`; the statement after
        // it starts on its own line.
        let dump = "-- PostgreSQL database dump\n\
                    \\restrict Qx9abc\n\
                    SET statement_timeout = 0;\n";
        let (mut tap, lines) = LineTap::new(Box::new(std::io::Cursor::new(dump)), true);
        std::io::copy(&mut tap, &mut std::io::sink()).expect("read");
        let lines = lines.lock().expect("lines");
        assert_eq!(lines.settings.iter().copied().collect::<Vec<_>>(), [3]);
    }

    #[test]
    fn a_line_with_two_statements_excuses_neither() {
        let dump = "GRANT SELECT ON t TO app; CREATE POLICY p ON t TO missing USING (true);\n\
                    GRANT SELECT ON t TO app;\n";
        let (mut tap, lines) = LineTap::new(Box::new(std::io::Cursor::new(dump)), true);
        std::io::copy(&mut tap, &mut std::io::sink()).expect("read");
        let lines = lines.lock().expect("lines");
        assert_eq!(
            lines.role_statements.iter().copied().collect::<Vec<_>>(),
            [2]
        );
    }

    #[test]
    fn a_mysql_dump_is_complete_only_with_its_end_line() {
        let noted = |dump: &'static str| {
            let (mut tap, lines) = LineTap::new(Box::new(std::io::Cursor::new(dump)), false);
            std::io::copy(&mut tap, &mut std::io::sink()).expect("read");
            let lines = lines.lock().expect("lines");
            (lines.made_by_dump_tool, lines.complete)
        };
        let whole = "-- MySQL dump 10.13  Distrib 8.4.0, for Linux (x86_64)\n\
                     CREATE TABLE t (id int);\n\
                     -- Dump completed on 2026-10-20 12:00:00\n";
        assert_eq!(noted(whole), (true, true));
        assert_eq!(
            noted("-- MySQL dump 10.13  Distrib 8.4.0\nCREATE TABLE t (id int);\n"),
            (true, false)
        );
    }

    #[test]
    fn a_mariadb_dump_needs_its_end_line_too() {
        let cut = "-- MariaDB dump 10.19  Distrib 10.11.6-MariaDB\nCREATE TABLE t (id int);\n";
        let (mut tap, lines) = LineTap::new(Box::new(std::io::Cursor::new(cut)), false);
        std::io::copy(&mut tap, &mut std::io::sink()).expect("read");
        let lines = lines.lock().expect("lines");
        assert!(lines.made_by_dump_tool && !lines.complete);
    }

    #[test]
    fn a_last_line_without_a_newline_is_still_noted() {
        let dump = "-- PostgreSQL database dump\nGRANT SELECT ON t TO r;\n-- PostgreSQL database dump complete";
        let (mut tap, lines) = LineTap::new(Box::new(std::io::Cursor::new(dump)), true);
        std::io::copy(&mut tap, &mut std::io::sink()).expect("read");
        let lines = lines.lock().expect("lines");
        assert!(lines.complete, "the unterminated end line counts");
        assert_eq!(
            lines.role_statements.iter().copied().collect::<Vec<_>>(),
            [2]
        );
    }

    /// The lines noted as role statements and settings.
    fn excused(dump: &str) -> (Vec<u64>, Vec<u64>) {
        let (mut tap, lines) = LineTap::new(Box::new(std::io::Cursor::new(dump.to_owned())), true);
        std::io::copy(&mut tap, &mut std::io::sink()).expect("read");
        let lines = lines.lock().expect("lines");
        (
            lines.role_statements.iter().copied().collect(),
            lines.settings.iter().copied().collect(),
        )
    }

    #[test]
    fn a_semicolon_inside_a_function_body_ends_no_statement() {
        // psql ends a statement only at a `;` outside quotes and bodies: the
        // body's own lines, and the line after a body's `;`, are inside it.
        let dump = "CREATE FUNCTION f() RETURNS void LANGUAGE plpgsql AS $$\n\
                    BEGIN\n\
                    SET LOCAL statement_timeout = 0;\n\
                    GRANT SELECT ON t TO app;\n\
                    END;\n\
                    $$;\n\
                    CREATE FUNCTION g() RETURNS int LANGUAGE sql AS $body$SELECT 1;\n\
                    SET LOCAL x = 1$body$;\n\
                    GRANT SELECT ON t TO app;\n";
        assert_eq!(excused(dump), (vec![9], vec![]));
    }

    #[test]
    fn a_dollar_quote_ends_only_at_its_own_tag() {
        let dump = "DO $outer$ BEGIN EXECUTE $$SELECT 1;$$; END $not$;\n\
                    GRANT SELECT ON t TO app;\n\
                    $outer$;\n\
                    GRANT SELECT ON t TO app;\n";
        assert_eq!(excused(dump), (vec![4], vec![]));
    }

    #[test]
    fn a_semicolon_inside_a_quoted_literal_ends_no_statement() {
        let dump = "INSERT INTO notes VALUES ('first;\n\
                    GRANT SELECT ON t TO app;\n\
                    ');\n\
                    INSERT INTO notes VALUES (E'it\\'s;\n\
                    SET x = 1;\n\
                    ', 'a''b;\n\
                    REVOKE ALL ON t FROM app;\n\
                    ');\n\
                    GRANT SELECT ON \"odd;name\" TO app;\n";
        assert_eq!(excused(dump), (vec![9], vec![]));
    }

    #[test]
    fn a_semicolon_inside_a_comment_ends_no_statement() {
        let dump = "SELECT 1/0 -- not the end;\n\
                    GRANT SELECT ON t TO app;\n\
                    SELECT 1 /* a /* nested */ comment;\n\
                    SET x = 1; */ + 1;\n\
                    SET x = 1; -- a comment after it\n";
        assert_eq!(excused(dump), (vec![], vec![5]));
    }

    #[test]
    fn a_semicolon_inside_parentheses_or_an_atomic_body_ends_no_statement() {
        // As psql's lexer: a rule's action list and a `BEGIN ATOMIC` body
        // hold their own `;`s; `CASE … END` elsewhere does not open a body.
        let dump = "CREATE RULE r AS ON INSERT TO t DO ALSO (NOTIFY t;\n\
                    GRANT SELECT ON t TO app;\n\
                    );\n\
                    CREATE OR REPLACE FUNCTION f() RETURNS int LANGUAGE sql\n\
                    BEGIN ATOMIC\n\
                    SELECT CASE WHEN true THEN 1 END;\n\
                    SET x = 1;\n\
                    END;\n\
                    SELECT CASE WHEN true THEN 1 END;\n\
                    GRANT SELECT ON t TO app;\n";
        assert_eq!(excused(dump), (vec![10], vec![]));
    }

    #[test]
    fn a_copy_inside_a_function_body_starts_no_data() {
        let dump = "CREATE FUNCTION g() RETURNS void LANGUAGE sql AS $$\n\
                    COPY public.users (id) FROM stdin;\n\
                    $$;\n\
                    GRANT SELECT ON t TO app;\n";
        let (mut tap, lines) = LineTap::new(Box::new(std::io::Cursor::new(dump)), true);
        std::io::copy(&mut tap, &mut std::io::sink()).expect("read");
        let lines = lines.lock().expect("lines");
        assert!(lines.copy_tables.is_empty(), "{:?}", lines.copy_tables);
        assert_eq!(
            lines.role_statements.iter().copied().collect::<Vec<_>>(),
            [4]
        );
    }

    #[test]
    fn a_dollar_that_opens_no_body_and_copy_data_are_not_quotes() {
        // `$1` is a parameter and `a$b` a name; `COPY` rows are not SQL.
        let dump = "PREPARE p AS SELECT $1, a$b FROM t;\n\
                    GRANT SELECT ON t TO app;\n\
                    COPY public.notes (body) FROM stdin;\n\
                    it's $$ open /* too\n\
                    \\.\n\
                    SET x = 1;\n";
        assert_eq!(excused(dump), (vec![2], vec![6]));
    }

    #[test]
    fn a_wide_tables_copy_header_past_the_kept_length_still_starts_its_data() {
        let columns = (0..1600)
            .map(|n| {
                format!("\"a_column_name_long_enough_to_take_the_header_past_the_kept_{n:04}\"")
            })
            .collect::<Vec<_>>()
            .join(", ");
        let dump = format!(
            "COPY public.wide ({columns}) FROM stdin;\nit's\n\\.\nGRANT SELECT ON t TO app;\n"
        );
        let (mut tap, lines) = LineTap::new(Box::new(std::io::Cursor::new(dump)), true);
        std::io::copy(&mut tap, &mut std::io::sink()).expect("read");
        let lines = lines.lock().expect("lines");
        assert_eq!(
            lines.copy_tables.get(&1).map(String::as_str),
            Some("public.wide")
        );
        assert_eq!(
            lines.role_statements.iter().copied().collect::<Vec<_>>(),
            [4]
        );
    }

    #[test]
    fn a_psql_semicolon_escape_or_separator_is_followed_or_given_up_on() {
        // `\;` puts a `;` into the query without ending it; `\\` hands the
        // rest of a command's line back to SQL, which is not followed.
        let escaped = "\\; CREATE SCHEMA s AUTHORIZATION missing CREATE VIEW v AS SELECT '\n\
                       GRANT SELECT ON t TO app; -- ' AS c;\n";
        assert_eq!(excused(escaped), (vec![], vec![]));
        let separated = "\\set x 1 \\\\ SELECT '\n\
                         GRANT SELECT ON t TO app; -- ';\n\
                         GRANT SELECT ON t TO app;\n";
        assert_eq!(excused(separated), (vec![], vec![]));
        // `\:` too: pg_dump writes neither.
        let colon = "SELECT 1 \\: 2;\nGRANT SELECT ON t TO app;\n";
        assert_eq!(excused(colon), (vec![], vec![]));
    }

    #[test]
    fn an_end_line_inside_a_literal_does_not_complete_the_dump() {
        let dump = "-- PostgreSQL database dump\n\
                    INSERT INTO notes VALUES ('\n\
                    -- PostgreSQL database dump complete\n\
                    CREATE TABLE _timescaledb_internal.x (t int);\n\
                    ');\n";
        let (mut tap, lines) = LineTap::new(Box::new(std::io::Cursor::new(dump)), true);
        std::io::copy(&mut tap, &mut std::io::sink()).expect("read");
        let lines = lines.lock().expect("lines");
        assert!(lines.made_by_dump_tool && !lines.complete && !lines.hypertables);
    }

    #[test]
    fn a_mysql_dumps_escaped_quotes_do_not_hide_its_end_line() {
        let dump = "-- MySQL dump 10.13  Distrib 8.4.0\n\
                    INSERT INTO t VALUES ('it\\'s');\n\
                    -- Dump completed on 2026-10-21 12:00:00\n";
        let (mut tap, lines) = LineTap::new(Box::new(std::io::Cursor::new(dump)), false);
        std::io::copy(&mut tap, &mut std::io::sink()).expect("read");
        let lines = lines.lock().expect("lines");
        assert!(lines.made_by_dump_tool && lines.complete);
    }

    #[test]
    fn only_a_statement_ending_owner_to_a_role_is_an_owner_change() {
        let dump = "ALTER TABLE public.t ADD CONSTRAINT c CHECK (s <> ' OWNER TO ');\n\
                    ALTER TABLE \"a OWNER TO b\" ADD COLUMN c int;\n\
                    ALTER TABLE public.t OWNER TO prod_app;\n\
                    ALTER SCHEMA \"My Schema\" OWNER TO \"Prod App\";\n\
                    ALTER TABLE t RENAME COLUMN owner TO owner_id;\n\
                    ALTER ROLE missing SET app.owner TO x;\n\
                    ALTER OPERATOR CLASS public.c USING btree OWNER TO prod_app;\n\
                    ALTER FUNCTION public.f(integer, text) OWNER TO prod_app;\n";
        assert_eq!(excused(dump), (vec![3, 4, 7, 8], vec![]));
    }

    #[test]
    fn an_escape_string_continues_past_a_doubled_quote() {
        let dump = "INSERT INTO t VALUES (E'a''\\';\n\
                    GRANT SELECT ON t TO app;\n\
                    ');\n\
                    SELECT (1));\n\
                    GRANT SELECT ON t TO app;\n";
        assert_eq!(excused(dump), (vec![5], vec![]));
    }

    #[test]
    fn an_owner_change_with_another_subcommand_is_no_owner_change() {
        let dump = "ALTER TABLE t ADD COLUMN c int, OWNER TO ghost;\n\
                    ALTER TABLE t OWNER TO app, ENABLE ROW LEVEL SECURITY;\n\
                    ALTER FUNCTION public.f(integer, text) OWNER TO prod_app;\n";
        assert_eq!(excused(dump), (vec![3], vec![]));
    }

    #[test]
    fn only_pg_dumps_own_setval_and_set_config_calls_are_noted() {
        let dump = "SELECT pg_catalog.setval('public.users_id_seq', 1, false);\n\
                    SELECT pg_catalog.setval('public.it''s_seq', -5, true);\n\
                    SELECT pg_catalog.setval('public.s', 1, false), f();\n\
                    SELECT pg_catalog.setval(f(), 1, false);\n\
                    SELECT pg_catalog.set_config('search_path', '', false);\n\
                    SELECT pg_catalog.set_config(g(), '', false);\n";
        let (mut tap, lines) = LineTap::new(Box::new(std::io::Cursor::new(dump)), true);
        std::io::copy(&mut tap, &mut std::io::sink()).expect("read");
        let lines = lines.lock().expect("lines");
        assert_eq!(
            lines.rowless_writes.iter().copied().collect::<Vec<_>>(),
            [1, 2]
        );
        assert_eq!(lines.settings.iter().copied().collect::<Vec<_>>(), [5]);
    }

    #[test]
    fn any_copy_from_stdin_starts_data() {
        // psql reads data after every spelling; only pg_dump's names its
        // table, and a header over several lines is not followed.
        let dump = "copy t from STDIN;\n\
                    1\tO'Brien\n\
                    \\.\n\
                    COPY t FROM stdin WITH (FORMAT csv);\n\
                    \"it's\",$$\n\
                    \\.\n\
                    GRANT SELECT ON t TO app;\n\
                    COPY t (a,\n\
                    b) FROM stdin;\n\
                    x'\n\
                    \\.\n\
                    GRANT SELECT ON t TO app;\n";
        let (mut tap, lines) = LineTap::new(Box::new(std::io::Cursor::new(dump)), true);
        std::io::copy(&mut tap, &mut std::io::sink()).expect("read");
        let lines = lines.lock().expect("lines");
        assert_eq!(
            lines.role_statements.iter().copied().collect::<Vec<_>>(),
            [7]
        );
        assert!(lines.copy_tables.is_empty() && lines.rowless_writes.is_empty());
    }

    #[test]
    fn only_pg_dumps_own_psql_commands_are_followed() {
        let kept = "\\restrict Qx9\nGRANT SELECT ON t TO app;\n\\unrestrict Qx9\n";
        assert_eq!(excused(kept), (vec![2], vec![]));
        for command in ["\\copy t from stdin", "\\i other.sql", "\\."] {
            let dump = format!("{command}\nGRANT SELECT ON t TO app;\n");
            assert_eq!(excused(&dump), (vec![], vec![]), "{command}");
        }
    }

    #[test]
    fn a_dump_that_switches_database_is_refused_at_its_connect() {
        // `pg_dump --create` would restore into the database it names.
        for connect in ["\\connect -reuse-previous=on \"dbname='prod'\"", "\\c prod"] {
            let dump = format!("CREATE DATABASE prod;\n{connect}\nCREATE TABLE t (id int);\n");
            let (mut tap, _lines) = LineTap::new(Box::new(std::io::Cursor::new(dump)), true);
            let mut passed = Vec::new();
            let error = std::io::copy(&mut tap, &mut passed).expect_err("refused");
            assert!(error.to_string().contains("line 2"), "{error}");
            assert!(
                !String::from_utf8_lossy(&passed).contains("CREATE TABLE"),
                "nothing after it reaches the client"
            );
        }
    }
    #[test]
    fn a_second_dump_must_reach_its_own_end_line() {
        let dump = "-- PostgreSQL database dump\n\
                    -- PostgreSQL database dump complete\n\
                    -- PostgreSQL database dump\n\
                    COPY public.t (id) FROM stdin;\n\
                    1\n";
        let (mut tap, lines) = LineTap::new(Box::new(std::io::Cursor::new(dump)), true);
        std::io::copy(&mut tap, &mut std::io::sink()).expect("read");
        let lines = lines.lock().expect("lines");
        assert!(lines.made_by_dump_tool && !lines.complete);
    }

    #[test]
    fn a_nul_byte_anywhere_fails_the_read() {
        let dump = format!(
            "{}SELECT 1;\0\nGRANT SELECT ON t TO app;\n",
            "-- x\n".repeat(4000)
        );
        let (mut tap, _lines) = LineTap::new(Box::new(std::io::Cursor::new(dump)), true);
        let error = std::io::copy(&mut tap, &mut std::io::sink()).expect_err("a NUL fails");
        assert!(error.to_string().contains("line 4001"), "{error}");
    }

    #[test]
    fn an_escape_string_continued_on_the_next_line_keeps_its_escapes() {
        // psqlscan.l's `quotecontinue`: `E'a'` and `'\';…` on the next line
        // are one escape string, so line 3's `'` closes it.
        let dump = "CREATE SCHEMA s AUTHORIZATION missing CREATE VIEW v AS SELECT E'a'\n\
                    '\\';\n\
                    GRANT SELECT ON t TO app; -- ' AS c;\n\
                    GRANT SELECT ON t TO app;\n";
        assert_eq!(excused(dump), (vec![4], vec![]));
    }

    #[test]
    fn a_copy_before_another_statement_on_its_line_still_starts_data() {
        let dump = "COPY t (a) FROM stdin; SELECT 1;\n\
                    CREATE TABLE _timescaledb_internal.not_a_chunk (row text);\n\
                    \\.\n";
        let (mut tap, lines) = LineTap::new(Box::new(std::io::Cursor::new(dump)), true);
        std::io::copy(&mut tap, &mut std::io::sink()).expect("read");
        let lines = lines.lock().expect("lines");
        assert!(!lines.hypertables, "its row is data, not a statement");
    }

    #[test]
    fn only_from_stdin_reads_data() {
        // A column named `stdin` in a `COPY … TO stdout` reads nothing: the
        // lines after it are SQL, and a `\connect` there is refused.
        let dump = "COPY (SELECT 1 AS stdin) TO stdout;\n\\set QUIET on\n\\connect other\n";
        let (mut tap, _lines) = LineTap::new(Box::new(std::io::Cursor::new(dump)), true);
        let error = std::io::copy(&mut tap, &mut std::io::sink()).expect_err("refused");
        assert!(error.to_string().contains("line 3"), "{error}");
    }

    #[test]
    fn a_mysql_dump_that_switches_database_is_refused() {
        for switch in [
            "USE `prod`;",
            "use prod;",
            "\\u prod",
            "connect prod",
            "USE `other db`;",
            "USE other; -- x",
            "connect other host",
            "\\r other host",
            "USE `app` `x`;",
            // Alone on its line (its name on the next), or in a comment or
            // a versioned comment.
            "USE",
            "USE/**/prod;",
            "/*!40000 USE prod */;",
        ] {
            let dump = format!(
                "-- MySQL dump 10.13\nCREATE DATABASE prod;\n{switch}\nCREATE TABLE t (id int);\n"
            );
            let (mut tap, _lines) = LineTap::new(Box::new(std::io::Cursor::new(dump)), false);
            tap.expect_database("app");
            let error = std::io::copy(&mut tap, &mut std::io::sink()).expect_err(switch);
            assert!(error.to_string().contains("line 3"), "{switch}: {error}");
        }
        let kept = "-- MySQL dump 10.13\nINSERT INTO t VALUES ('use the force');\n\
                    INSERT INTO t VALUES ('a\\\\\\\\u x'); USE app;\nSELECT 1; -- used\n\
                    INSERT INTO t VALUES ('fresh; use within 3 days', 'O\\'Brien; use x');\n\
                    /* a comment; use other */ SELECT `a; use other`;\n\
                    DELIMITER ;;\nCREATE PROCEDURE p() BEGIN SELECT 1; END ;;\nDELIMITER ;\n\
                    -- Dump completed\n";
        let (mut tap, _lines) = LineTap::new(Box::new(std::io::Cursor::new(kept)), false);
        tap.expect_database("app");
        std::io::copy(&mut tap, &mut std::io::sink()).expect("a line that only says use");
    }

    #[test]
    fn a_dump_that_sets_on_error_rollback_is_refused() {
        for command in [
            "\\set ON_ERROR_ROLLBACK off",
            "\\unset ON_ERROR_ROLLBACK",
            "\\set  ON_ERROR_ROLLBACK off",
            "\\set\tON_ERROR_ROLLBACK off",
            "\\set :name off",
        ] {
            let dump = format!("SELECT 1;\n{command}\nBEGIN;\n");
            let (mut tap, _lines) = LineTap::new(Box::new(std::io::Cursor::new(dump)), true);
            let error = std::io::copy(&mut tap, &mut std::io::sink()).expect_err(command);
            assert!(error.to_string().contains("line 2"), "{command}: {error}");
        }
    }

    #[test]
    fn a_dump_that_hides_a_command_from_the_tap_excuses_nothing_after_it() {
        // With `standard_conforming_strings` off, psql takes `\'` as an
        // escaped quote: line 2 ends there, and line 3's `\gset` (setting
        // `ON_ERROR_ROLLBACK` off) runs, while a reading with it on is still
        // inside the string. The grant's error then aborts the transaction
        // and COMMIT rolls it back: it must not be excused.
        let hidden = r#"SET standard_conforming_strings = off;
SELECT 'a\'';
SELECT 'off' AS "ON_ERROR_ROLLBACK" \gset
-- ';
BEGIN;
CREATE TABLE important (id int);
GRANT SELECT ON important TO missing_role;
COMMIT;
"#;
        // The last error's text is the dump's (`RAISE`), and psql lexes it
        // where `:LAST_ERROR_MESSAGE` stands.
        let interpolated = "\
DO $$BEGIN RAISE EXCEPTION 'x'; END$$;
SELECT :LAST_ERROR_MESSAGE;
GRANT SELECT ON important TO missing_role;
";
        let after_a_cast = "\
DO $$BEGIN RAISE EXCEPTION 'x'; END$$;
SELECT 1:::LAST_ERROR_MESSAGE;
GRANT SELECT ON important TO missing_role;
";
        for (dump, grant) in [(hidden, 7), (interpolated, 3), (after_a_cast, 3)] {
            let (mut tap, lines) = LineTap::new(Box::new(std::io::Cursor::new(dump)), true);
            std::io::copy(&mut tap, &mut std::io::sink()).expect("read");
            let lines = lines.lock().expect("lines");
            assert!(!lines.role_statements.contains(&grant), "{dump}");
        }
        // Either reading closes these strings at the same quote.
        let plain = r#"COMMENT ON TABLE t IS 'C:\path\to';
COMMENT ON TABLE u IS 'a\\';
SELECT ':LAST_ERROR_MESSAGE', last_error_message::last_error_message, 1::::LAST_ERROR_MESSAGE FROM t;
GRANT SELECT ON t TO app;
"#;
        let (mut tap, lines) = LineTap::new(Box::new(std::io::Cursor::new(plain)), true);
        std::io::copy(&mut tap, &mut std::io::sink()).expect("read");
        let lines = lines.lock().expect("lines");
        assert!(lines.role_statements.contains(&4));
    }

    #[test]
    fn a_dump_that_sets_on_error_rollback_inside_what_reads_as_a_string_is_refused() {
        let dump = r#"SET standard_conforming_strings = off;
SELECT 'a\'';
\set ON_ERROR_ROLLBACK off
-- ';
"#;
        let (mut tap, _lines) = LineTap::new(Box::new(std::io::Cursor::new(dump)), true);
        let error = std::io::copy(&mut tap, &mut std::io::sink()).expect_err("refused");
        assert!(error.to_string().contains("line 3"), "{error}");
    }

    fn mysql_tap(dump: &str) -> std::io::Result<u64> {
        let (mut tap, _lines) =
            LineTap::new(Box::new(std::io::Cursor::new(dump.to_owned())), false);
        tap.expect_database("app");
        std::io::copy(&mut tap, &mut std::io::sink())
    }

    #[test]
    fn a_nul_byte_anywhere_in_a_mysql_dump_is_refused() {
        let dump = format!(
            "{}\nINSERT INTO t VALUES ('a\0b');\n",
            "-- x\n".repeat(4000)
        );
        let error = mysql_tap(&dump).expect_err("NUL");
        assert!(
            error.to_string().contains("NUL byte on line 4002"),
            "{error}"
        );
    }

    #[test]
    fn a_mysql_dump_may_use_its_own_database() {
        // `mysqldump --databases app` into the service's `app` is no switch,
        // nor is setting the prompt (`\R`).
        let dump = "-- MySQL dump 10.13\nCREATE DATABASE IF NOT EXISTS `app`;\nUSE `app`; -- note\nuse app\n\\R mysql> \n-- Dump completed\n";
        let (mut tap, _lines) = LineTap::new(Box::new(std::io::Cursor::new(dump)), false);
        tap.expect_database("app");
        std::io::copy(&mut tap, &mut std::io::sink()).expect("its own database");
    }
}
