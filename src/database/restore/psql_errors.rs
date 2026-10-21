//! The statements `psql` reported as failed while it ran a dump.
//!
//! psql runs every statement of a dump and exits 0 whatever failed, so a
//! restore reads its errors instead of its exit code, as they arrive (a
//! restore can print one per row). It runs with `-f -`, so each error
//! names its line: `psql:<stdin>:12: ERROR:  …`.
//!
//! Some errors leave the data whole and are warnings: a role or owner error
//! on a statement that only sets an owner, a grant, default privileges, the
//! session's authorization (`--use-set-session-authorization`) or an
//! extension's comment (a dump from production or another instance names roles this
//! server lacks), a setting an older server does not have, and a denied
//! write to an extension's own table or sequence that carries no rows (its
//! catalog, which a tenant may not write). Which statement a line holds
//! comes from [`StatementLines`], and psql's echo of the failed statement
//! (`--echo-errors`, `psql:<stdin>:12: STATEMENT:  …`) must be that line:
//! the line tap follows psql's lexer without running it, so a line it
//! misreads excuses nothing. Any other error is a failed restore: a role
//! error on a policy or constraint leaves it missing, and a denied `COPY`
//! with rows loses them.

use std::collections::BTreeSet;

use super::statement_lines::{StatementLines, TextKey, text_key};

/// How many failed statements are kept to name.
pub(super) const SHOWN: usize = 5;

/// Most messages kept per kind of warning: past it, an error keeps only its
/// line (a dump from production can hold an owner or grant error per
/// object), and one that turns out to fail is named by its kind.
const MAX_KEPT: usize = 10_000;

/// One reported error: its line in the dump, when psql named it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Failure {
    pub(super) line: Option<u64>,
    pub(super) message: String,
    /// psql's own error (`error:`), not the server's (`ERROR:`): a
    /// `\copy` or `\i` whose file is missing, or an unknown command.
    pub(super) from_psql: bool,
}

impl std::fmt::Display for Failure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.line {
            Some(line) => write!(formatter, "line {line}: {}", self.message),
            None => formatter.write_str(&self.message),
        }
    }
}

/// An error judged once the restore ends: its line, its message while
/// fewer than [`MAX_KEPT`] of its kind were seen, and the statement psql
/// echoed for it, with the line it echoed it on, when that fit on one line.
#[derive(Debug)]
struct Pending {
    line: Option<u64>,
    kept: Option<Failure>,
    statement: Option<(u64, TextKey)>,
}

impl Pending {
    fn new(failure: Failure, seen: usize) -> Self {
        Self {
            line: failure.line,
            kept: (seen < MAX_KEPT).then_some(failure),
            statement: None,
        }
    }

    /// psql's echo of the failed statement is the dump's line `line`,
    /// echoed on that line, or on its `COPY`'s `\.` (psql 15+ reports a
    /// refused `COPY` on its own line and echoes it once past the data it
    /// skipped; 13 and 14 echo it on its own line).
    fn echoes(&self, statements: &StatementLines, line: u64) -> bool {
        self.statement.as_ref().is_some_and(|(echoed_on, text)| {
            Some(text) == statements.texts.get(&line)
                && (*echoed_on == line || statements.copy_ends.get(&line) == Some(echoed_on))
        })
    }

    fn into_failure(self, kind: impl FnOnce() -> String) -> Failure {
        self.kept.unwrap_or_else(|| Failure {
            line: self.line,
            message: kind(),
            from_psql: false,
        })
    }
}

/// psql's errors so far, by kind.
#[derive(Debug, Default)]
pub(super) struct PsqlErrors {
    /// Role and owner errors, judged by their statement.
    roles: Vec<Pending>,
    /// Settings this server does not have, judged by their statement.
    settings: Vec<Pending>,
    /// psql's own lines berth knows to be harmless (`\restrict`).
    tolerated: usize,
    /// `permission denied for table|sequence <name>`, by the name's index in
    /// `denied_names`, to look up.
    denied: Vec<(usize, Pending)>,
    denied_names: Vec<String>,
    /// psql's error on a `COPY`'s `\.` line: a psql without the CVE-2026-6464
    /// fix (before 18.6, 17.11, 16.15, 15.19, 14.24; all of 13) runs a
    /// refused `COPY … FROM stdin`'s rows and terminator as its own input.
    copy_ends: Vec<Pending>,
    /// The first [`SHOWN`] other errors.
    failed: Vec<Failure>,
    failed_total: usize,
    /// The error psql's next `STATEMENT:` echo belongs to, and whether that
    /// echo is being read (its continuation lines carry no `psql:`).
    awaiting: Option<Awaiting>,
    echoing: bool,
    /// The error the last echo was for: psql echoes once per error, so a
    /// second echo before the next error is a line of the first's
    /// statement, or of the error's own text.
    echoed: Option<Awaiting>,
}

/// Which kept error a `STATEMENT:` echo is for.
#[derive(Debug, Clone, Copy)]
struct Awaiting {
    kind: Kind,
    index: usize,
    line: u64,
}

#[derive(Debug, Clone, Copy)]
enum Kind {
    Role,
    Setting,
    Denied,
}

/// The errors once denied tables are sorted into extension tables or not.
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct Verdict {
    pub(super) roles: usize,
    pub(super) settings: usize,
    pub(super) extension_tables: usize,
    pub(super) failed: Vec<Failure>,
    pub(super) failed_total: usize,
    /// Failed `\.` lines (either psql wording): data a refused COPY sent ran
    /// as commands, which a psql without the CVE-2026-6464 fix does.
    pub(super) stray_terminators: usize,
}

impl PsqlErrors {
    /// Takes one line of psql's stderr.
    pub(super) fn observe(&mut self, line: &str) {
        if self.echoing && !line.starts_with("psql:") {
            // The echoed statement goes on: it is not one line.
            self.forget_echo();
            return;
        }
        self.echoing = false;
        if let Some((number, statement)) = statement_echo(line) {
            if self.echoed.is_some() {
                self.forget_echo();
            } else if let Some(awaiting) = self.awaiting.take()
                && number >= awaiting.line
                && let Some(pending) = self.pending(awaiting)
            {
                pending.statement = Some((number, text_key(statement)));
                self.echoed = Some(awaiting);
                self.echoing = true;
            }
            return;
        }
        // An error's `DETAIL`, `HINT` and `LINE` context come between it and
        // its echo.
        let Some(failure) = parse(line) else {
            return;
        };
        // psql echoes only the server's errors, so only one of those starts
        // another error's window; its own (a `\restrict` it does not know)
        // must not end the one-echo rule of the error before it.
        if !failure.from_psql {
            self.awaiting = None;
            self.echoed = None;
        }
        let message = failure.message.as_str();
        if failure.from_psql {
            // pg_dump 17.6+ fences its dump with `\restrict`, which an older
            // psql does not know; skipping the fence changes no data.
            if message.contains("invalid command \\restrict")
                || message.contains("invalid command \\unrestrict")
            {
                self.tolerated += 1;
            } else if message.contains("invalid command \\.")
                || message.starts_with("backslash commands are restricted")
            {
                let pending = Pending::new(failure, self.copy_ends.len());
                self.copy_ends.push(pending);
            } else {
                self.fail(failure);
            }
        } else if is_role_error(message) {
            let line = failure.line;
            let pending = Pending::new(failure, self.roles.len());
            self.roles.push(pending);
            self.await_echo(Kind::Role, self.roles.len() - 1, line);
        } else if message.starts_with("unrecognized configuration parameter") {
            let line = failure.line;
            let pending = Pending::new(failure, self.settings.len());
            self.settings.push(pending);
            self.await_echo(Kind::Setting, self.settings.len() - 1, line);
        } else if let Some(name) = denied_relation(message) {
            let index = match self.denied_names.iter().position(|known| known == name) {
                Some(index) => index,
                None => {
                    self.denied_names.push(name.to_owned());
                    self.denied_names.len() - 1
                }
            };
            let line = failure.line;
            let pending = Pending::new(failure, self.denied.len());
            self.denied.push((index, pending));
            self.await_echo(Kind::Denied, self.denied.len() - 1, line);
        } else {
            self.fail(failure);
        }
    }

    fn await_echo(&mut self, kind: Kind, index: usize, line: Option<u64>) {
        self.awaiting = line.map(|line| Awaiting { kind, index, line });
    }

    fn pending(&mut self, awaiting: Awaiting) -> Option<&mut Pending> {
        match awaiting.kind {
            Kind::Role => self.roles.get_mut(awaiting.index),
            Kind::Setting => self.settings.get_mut(awaiting.index),
            Kind::Denied => self
                .denied
                .get_mut(awaiting.index)
                .map(|(_, pending)| pending),
        }
    }

    fn forget_echo(&mut self) {
        if let Some(echoed) = self.echoed
            && let Some(pending) = self.pending(echoed)
        {
            pending.statement = None;
        }
    }

    /// The tables and sequences a permission error named, schema-qualified
    /// where its line is a `COPY` (see [`denied_key`]).
    pub(super) fn denied_names(&self, statements: &StatementLines) -> BTreeSet<String> {
        self.denied
            .iter()
            .map(|(index, pending)| denied_key(&self.denied_names[*index], pending, statements))
            .collect()
    }

    /// Judges the role errors by `statements` and the denied writes by
    /// `extension_tables` (the names the database's extensions own).
    pub(super) fn verdict(
        mut self,
        statements: &StatementLines,
        extension_tables: &BTreeSet<String>,
    ) -> Verdict {
        let mut roles = 0;
        for pending in std::mem::take(&mut self.roles) {
            if pending.line.is_some_and(|line| {
                statements.role_statements.contains(&line) && pending.echoes(statements, line)
            }) {
                roles += 1;
            } else {
                self.fail(pending.into_failure(|| "a role or owner error".to_owned()));
            }
        }
        let mut settings = 0;
        for pending in std::mem::take(&mut self.settings) {
            if pending.line.is_some_and(|line| {
                statements.settings.contains(&line) && pending.echoes(statements, line)
            }) {
                settings += 1;
            } else {
                // A function or role defined with a setting this server
                // lacks is not created.
                self.fail(
                    pending.into_failure(|| "unrecognized configuration parameter".to_owned()),
                );
            }
        }
        let mut extension = 0;
        let names = std::mem::take(&mut self.denied_names);
        let mut skipped_copies = BTreeSet::new();
        for (index, pending) in std::mem::take(&mut self.denied) {
            let name = &denied_key(&names[index], &pending, statements);
            let harmless = pending.line.is_some_and(|line| {
                (statements.rowless_writes.contains(&line)
                    || statements.bookkeeping_copies.contains(&line))
                    && pending.echoes(statements, line)
            });
            if extension_tables.contains(name) && harmless {
                extension += 1;
                // Only a COPY has a `\.` after it.
                skipped_copies.extend(
                    pending
                        .line
                        .filter(|line| statements.copy_tables.contains_key(line)),
                );
            } else {
                self.fail(pending.into_failure(|| format!("permission denied for {name}")));
            }
        }
        // A skipped row-less `COPY`'s terminator is its next line; any other
        // stray `\.` means data ran as commands.
        let mut stray_terminators = 0;
        for pending in std::mem::take(&mut self.copy_ends) {
            if pending
                .line
                .is_some_and(|line| line > 1 && skipped_copies.contains(&(line - 1)))
            {
                continue;
            }
            stray_terminators += 1;
            self.fail(pending.into_failure(|| "invalid command \\.".to_owned()));
        }
        Verdict {
            roles,
            settings,
            extension_tables: extension,
            failed: self.failed,
            failed_total: self.failed_total,
            stray_terminators,
        }
    }

    fn fail(&mut self, failure: Failure) {
        self.failed_total += 1;
        if self.failed.len() < SHOWN {
            self.failed.push(failure);
        }
    }
}

/// What a denied relation is looked up by: the `COPY` line's own table
/// (`schema.table`) when the error is on one, so a table of the database's
/// own with the same bare name (a `public.metadata`) cannot hide the
/// extension's; otherwise the bare name the error gives.
fn denied_key(name: &str, pending: &Pending, statements: &StatementLines) -> String {
    pending
        .line
        .and_then(|line| statements.copy_tables.get(&line))
        .cloned()
        .unwrap_or_else(|| name.to_owned())
}

/// psql's echo of a failed statement (`-b`, `--echo-errors`):
/// `psql:<stdin>:12: STATEMENT:  GRANT …;`, the statement as sent (its
/// trailing comment not), more lines following without a prefix when it
/// spans several.
fn statement_echo(line: &str) -> Option<(u64, &str)> {
    let (number, statement) = line
        .strip_prefix("psql:<stdin>:")?
        .split_once(": STATEMENT:  ")?;
    if number.is_empty() || !number.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    Some((number.parse().ok()?, statement))
}

/// The levels psql prefixes a message with that are no error: the server's
/// (in English, as the restore asks for) and psql's own (12+).
const NOT_ERRORS: [&str; 13] = [
    "WARNING",
    "NOTICE",
    "INFO",
    "LOG",
    "DEBUG",
    "DETAIL",
    "HINT",
    "CONTEXT",
    "STATEMENT",
    "LINE",
    "warning",
    "detail",
    "hint",
];

/// The levels psql prefixes an error with.
const ERRORS: [&str; 5] = ["ERROR", "FATAL", "PANIC", "error", "fatal"];

/// An error line: the server's (`ERROR:`, `FATAL:`) or psql's own
/// (`error:`), with its dump line when psql names it.
fn parse(line: &str) -> Option<Failure> {
    let (line_number, rest) = match line.strip_prefix("psql:") {
        Some(located) => {
            // `psql:<stdin>:12: ERROR:  …` or `psql: error: …`: the level is
            // the first known one after a `: ` (a NOTICE quoting `ERROR:` is
            // a notice). A line with no known level fails closed: psql 11
            // prints its own errors without one (`psql:<stdin>:3: rows.csv:
            // No such file or directory`), and a server that ignored the
            // English it was asked for translates its levels.
            // On a dump line, the level is the very next segment: a later
            // one (`x: NOTICE: y.csv: No such file`) is part of the message.
            if let Some((number, rest)) = located
                .strip_prefix("<stdin>:")
                .and_then(|rest| rest.split_once(": "))
                && !number.is_empty()
                && number.bytes().all(|byte| byte.is_ascii_digit())
            {
                let level = rest.split_once(':').map_or("", |(level, _)| level);
                if NOT_ERRORS.contains(&level) {
                    return None;
                }
                let known = ERRORS.contains(&level);
                let message = if known {
                    rest.split_once(':').map_or(rest, |(_, message)| message)
                } else {
                    rest
                };
                return Some(Failure {
                    line: number.parse().ok(),
                    message: shown(message),
                    from_psql: !known || level == "error" || level == "fatal",
                });
            }
            let found = located.match_indices(": ").find_map(|(at, _)| {
                let (level, message) = located[at + 2..].split_once(':')?;
                (ERRORS.contains(&level) || NOT_ERRORS.contains(&level))
                    .then(|| (&located[..at], level, message))
            });
            let (location, level, message) = match found {
                Some((_, level, _)) if NOT_ERRORS.contains(&level) => return None,
                Some(found) => found,
                None => {
                    let (number, message) = located
                        .strip_prefix("<stdin>:")
                        .and_then(|rest| rest.split_once(": "))
                        .map_or((None, located), |(number, message)| {
                            (number.parse().ok(), message)
                        });
                    return Some(Failure {
                        line: number,
                        message: shown(message),
                        from_psql: true,
                    });
                }
            };
            // Only the dump's own lines (`<stdin>`) are judged by their
            // statement; an error in a file it includes (`\i`) names that
            // file's line, which says nothing about the dump's.
            let number = location
                .strip_prefix("<stdin>:")
                .and_then(|number| number.parse().ok());
            let tag: &&str = if level == "error" || level == "fatal" {
                &"error:"
            } else {
                &"ERROR:"
            };
            (number, (tag, message))
        }
        None => (
            None,
            ["ERROR:", "FATAL:"]
                .iter()
                .find_map(|tag| line.strip_prefix(tag).map(|rest| (tag, rest)))?,
        ),
    };
    let (tag, message) = rest;
    Some(Failure {
        line: line_number,
        message: shown(message),
        from_psql: *tag == "error:",
    })
}

/// The most of an error's message kept to name it: a dump's own `RAISE`
/// can print one of any length.
const SHOWN_MESSAGE: usize = 2048;

fn shown(message: &str) -> String {
    let message = message.trim();
    match message.char_indices().nth(SHOWN_MESSAGE) {
        Some((end, _)) => format!("{}…", &message[..end]),
        None => message.to_owned(),
    }
}

/// PostgreSQL's wording for an owner, grant or comment the role may not
/// set (`ALTER … OWNER TO`, `GRANT`, `REVOKE`, `COMMENT ON EXTENSION`).
fn is_role_error(message: &str) -> bool {
    (message.starts_with("role \"") && message.ends_with("does not exist"))
        || message.starts_with("must be owner of")
        || message.starts_with("must be able to SET ROLE")
        || message.starts_with("must be member of role")
        || message.starts_with("permission denied to grant")
        || message.starts_with("permission denied to change owner")
        // PostgreSQL 16+ (`aclchk.c`) for `ALTER DEFAULT PRIVILEGES FOR ROLE`
        // a role the restoring one may not act as; 15 says `must be member`.
        || message == "permission denied to change default privileges"
}

/// The relation a `permission denied for table|sequence <name>` names.
fn denied_relation(message: &str) -> Option<&str> {
    message
        .strip_prefix("permission denied for table ")
        .or_else(|| message.strip_prefix("permission denied for sequence "))
        .map(str::trim)
}

#[cfg(test)]
mod tests {
    use super::super::statement_lines::{StatementLines, text_key};
    use super::{Failure, PsqlErrors, Verdict};
    use std::collections::BTreeSet;

    fn statements() -> StatementLines {
        texts(StatementLines {
            role_statements: BTreeSet::from([12, 33]),
            settings: BTreeSet::from([14]),
            rowless_writes: BTreeSet::from([68, 69]),
            ..StatementLines::default()
        })
    }

    /// Gives every noted line a text, as the line tap does.
    fn texts(mut lines: StatementLines) -> StatementLines {
        let noted: Vec<u64> = lines
            .role_statements
            .iter()
            .chain(&lines.settings)
            .chain(&lines.rowless_writes)
            .chain(&lines.bookkeeping_copies)
            .chain(lines.copy_tables.keys())
            .copied()
            .collect();
        for line in noted {
            lines
                .texts
                .insert(line, text_key(&format!("statement on line {line};")));
        }
        for &copy in lines.copy_tables.keys() {
            lines.copy_ends.entry(copy).or_insert(copy + 1);
        }
        lines
    }

    /// psql's stderr for `input`: each server error on a line of `lines` is
    /// followed by its echo (`--echo-errors`), that line's statement.
    fn observed<I: AsRef<str>>(
        lines: &StatementLines,
        input: impl IntoIterator<Item = I>,
    ) -> PsqlErrors {
        let mut errors = PsqlErrors::default();
        for line in input {
            let line = line.as_ref();
            errors.observe(line);
            if let Some((number, _)) = line
                .strip_prefix("psql:<stdin>:")
                .and_then(|rest| rest.split_once(": ERROR:"))
                && let Ok(number) = number.parse::<u64>()
                && lines.texts.contains_key(&number)
            {
                let text = format!("statement on line {number};");
                // psql echoes a refused COPY past its data, on its `\.`.
                let echoed_on = lines.copy_ends.get(&number).copied().unwrap_or(number);
                errors.observe(&format!("psql:<stdin>:{echoed_on}: STATEMENT:  {text}"));
            }
        }
        errors
    }

    #[test]
    fn errors_are_sorted_into_warnings_and_failures() {
        let errors = observed(
            &statements(),
            [
                "psql:<stdin>:12: ERROR:  role \"prod_app\" does not exist",
                "psql:<stdin>:33: ERROR:  must be owner of extension timescaledb",
                "psql:<stdin>:14: ERROR:  unrecognized configuration parameter \"transaction_timeout\"",
                "psql:<stdin>:68: ERROR:  permission denied for table hypertable",
                "psql:<stdin>:69: ERROR:  permission denied for table users",
                "psql:<stdin>:90: ERROR:  relation \"orders\" already exists",
                "NOTICE:  a notice that mentions ERROR: is not one",
                "ERROR:  syntax error at or near \"FOO\"",
            ],
        );
        assert_eq!(
            errors.denied_names(&statements()),
            BTreeSet::from(["hypertable".to_owned(), "users".to_owned()])
        );
        let verdict = errors.verdict(&statements(), &BTreeSet::from(["hypertable".to_owned()]));
        assert_eq!(
            verdict,
            Verdict {
                roles: 2,
                settings: 1,
                extension_tables: 1,
                failed: vec![
                    Failure {
                        line: Some(90),
                        message: "relation \"orders\" already exists".to_owned(),
                        from_psql: false,
                    },
                    Failure {
                        line: None,
                        message: "syntax error at or near \"FOO\"".to_owned(),
                        from_psql: false,
                    },
                    Failure {
                        line: Some(69),
                        message: "permission denied for table users".to_owned(),
                        from_psql: false,
                    },
                ],
                failed_total: 3,
                stray_terminators: 0,
            }
        );
        assert_eq!(
            verdict.failed[0].to_string(),
            "line 90: relation \"orders\" already exists"
        );
    }

    #[test]
    fn a_role_error_on_another_statement_or_a_denied_copy_with_rows_fails() {
        // A policy naming a missing role is not created; an extension
        // catalog COPY with rows loses them.
        let errors = observed(
            &statements(),
            [
                "psql:<stdin>:50: ERROR:  role \"app_role\" does not exist",
                "psql:<stdin>:70: ERROR:  permission denied for table hypertable",
            ],
        );
        let verdict = errors.verdict(&statements(), &BTreeSet::from(["hypertable".to_owned()]));
        assert_eq!(verdict.roles, 0);
        assert_eq!(verdict.extension_tables, 0);
        assert_eq!(verdict.failed_total, 2, "{verdict:?}");
    }

    #[test]
    fn psqls_own_errors_fail_except_the_restrict_fence() {
        let mut errors = PsqlErrors::default();
        errors.observe("psql:<stdin>:3: error: users.csv: No such file or directory");
        errors.observe("psql:<stdin>:1: error: invalid command \\restrict");
        // A function using a setting this server lacks is not created.
        errors.observe(
            "psql:<stdin>:40: ERROR:  unrecognized configuration parameter \"transaction_timeout\"",
        );
        // A denied INSERT carries rows.
        errors.observe("psql:<stdin>:50: ERROR:  permission denied for table spatial_ref_sys");
        let verdict = errors.verdict(
            &statements(),
            &BTreeSet::from(["spatial_ref_sys".to_owned()]),
        );
        assert_eq!(verdict.failed_total, 3, "{verdict:?}");
        assert!(verdict.failed[0].from_psql);
        assert_eq!(verdict.settings, 0);
        assert_eq!(verdict.extension_tables, 0);
    }

    #[test]
    fn an_old_psqls_stray_terminator_after_a_skipped_empty_copy_is_no_failure() {
        // psql 13 (no CVE-2026-6464 fix) after a refused empty `COPY … FROM
        // stdin`, as it prints it.
        let lines = texts(StatementLines {
            rowless_writes: BTreeSet::from([68, 70]),
            copy_tables: std::collections::BTreeMap::from([
                (68, "_timescaledb_catalog.hypertable".to_owned()),
                (70, "public.users".to_owned()),
            ]),
            ..StatementLines::default()
        });
        let errors = observed(
            &lines,
            [
                "psql:<stdin>:68: ERROR:  permission denied for table hypertable",
                "psql:<stdin>:69: error: invalid command \\.",
                "psql:<stdin>:70: ERROR:  permission denied for table users",
                "psql:<stdin>:71: error: invalid command \\.",
            ],
        );
        let verdict = errors.verdict(
            &lines,
            &BTreeSet::from(["_timescaledb_catalog.hypertable".to_owned()]),
        );
        assert_eq!(verdict.extension_tables, 1);
        // `users` is no extension's: its denial and its terminator fail.
        assert_eq!(verdict.failed_total, 2, "{verdict:?}");
        assert_eq!(
            verdict.failed[1].to_string(),
            "line 71: invalid command \\."
        );
    }

    #[test]
    fn a_restricted_psqls_stray_terminator_is_judged_as_an_old_psqls() {
        // An unfixed psql inside pg_dump's `\restrict` fence words the `\.`
        // after a refused COPY as a restricted command.
        let restricted = "error: backslash commands are restricted; only \\unrestrict is allowed";
        let lines = texts(StatementLines {
            rowless_writes: BTreeSet::from([68]),
            copy_tables: std::collections::BTreeMap::from([(
                68,
                "_timescaledb_catalog.hypertable".to_owned(),
            )]),
            ..StatementLines::default()
        });
        let errors = observed(
            &lines,
            [
                "psql:<stdin>:68: ERROR:  permission denied for table hypertable".to_owned(),
                format!("psql:<stdin>:69: {restricted}"),
                format!("psql:<stdin>:90: {restricted}"),
            ],
        );
        let verdict = errors.verdict(
            &lines,
            &BTreeSet::from(["_timescaledb_catalog.hypertable".to_owned()]),
        );
        assert_eq!(verdict.extension_tables, 1);
        // Line 69 follows the skipped COPY; line 90 follows nothing.
        assert_eq!(verdict.failed_total, 1, "{verdict:?}");
        assert_eq!(verdict.stray_terminators, 1);
    }

    #[test]
    fn a_stray_terminator_after_a_skipped_setval_fails() {
        // Only a COPY is followed by `\.`: one after a `setval` means data
        // ran as commands.
        let lines = texts(StatementLines {
            rowless_writes: BTreeSet::from([68]),
            ..StatementLines::default()
        });
        let errors = observed(
            &lines,
            [
                "psql:<stdin>:68: ERROR:  permission denied for sequence chunk_id_seq",
                "psql:<stdin>:69: error: invalid command \\.",
            ],
        );
        let verdict = errors.verdict(&lines, &BTreeSet::from(["chunk_id_seq".to_owned()]));
        assert_eq!(verdict.extension_tables, 1);
        assert_eq!(verdict.failed_total, 1, "{verdict:?}");
    }

    #[test]
    fn timescaledbs_metadata_rows_are_skipped_but_other_catalog_rows_fail() {
        // A tenant restoring a dump of a database with TimescaleDB, as psql 18
        // reports it (lines from a real dump).
        let lines = texts(StatementLines {
            bookkeeping_copies: BTreeSet::from([213]),
            copy_tables: std::collections::BTreeMap::from([
                (213, "_timescaledb_catalog.metadata".to_owned()),
                (77, "_timescaledb_catalog.bgw_job".to_owned()),
            ]),
            ..StatementLines::default()
        });
        let errors = observed(
            &lines,
            [
                "psql:<stdin>:213: ERROR:  permission denied for table metadata",
                "psql:<stdin>:77: ERROR:  permission denied for table bgw_job",
            ],
        );
        // Looked up by the COPY's own table: an app's `public.metadata` of
        // the same bare name does not hide the extension's.
        assert_eq!(
            errors.denied_names(&lines),
            BTreeSet::from([
                "_timescaledb_catalog.bgw_job".to_owned(),
                "_timescaledb_catalog.metadata".to_owned(),
            ])
        );
        let extension = BTreeSet::from([
            "_timescaledb_catalog.metadata".to_owned(),
            "_timescaledb_catalog.bgw_job".to_owned(),
        ]);
        let verdict = errors.verdict(&lines, &extension);
        assert_eq!(verdict.extension_tables, 1);
        // A job's rows are the dump's own data: losing them fails.
        assert_eq!(verdict.failed_total, 1, "{verdict:?}");
        assert_eq!(verdict.failed[0].line, Some(77));
    }

    #[test]
    fn a_dump_lines_level_is_the_segment_right_after_its_number() {
        let mut errors = PsqlErrors::default();
        // psql 11's own error about a file whose name holds `NOTICE:`.
        errors.observe("psql:<stdin>:3: x: NOTICE: y.csv: No such file or directory");
        errors.observe("psql:<stdin>:4: NOTICE:  relation \"t\" already exists, skipping");
        let verdict = errors.verdict(&statements(), &BTreeSet::new());
        assert_eq!(verdict.failed_total, 1, "{verdict:?}");
        assert_eq!(verdict.failed[0].line, Some(3));
    }

    #[test]
    fn postgres_16s_default_privileges_refusal_is_a_role_warning_on_its_line_only() {
        let errors = observed(
            &statements(),
            [
                "psql:<stdin>:12: ERROR:  permission denied to change default privileges",
                "psql:<stdin>:50: ERROR:  permission denied to change default privileges",
            ],
        );
        let verdict = errors.verdict(&statements(), &BTreeSet::new());
        assert_eq!(verdict.roles, 1);
        assert_eq!(verdict.failed_total, 1, "{verdict:?}");
    }

    #[test]
    fn a_line_without_a_known_level_fails_closed() {
        let mut errors = PsqlErrors::default();
        // psql 11's own error: no `error:` word.
        errors.observe("psql:<stdin>:3: rows.csv: No such file or directory");
        // A server that answered in German.
        errors.observe("psql:<stdin>:12: FEHLER:  Rolle »app« existiert nicht");
        // psql 12+'s own notes and the server's English notices pass.
        errors.observe("psql:<stdin>:4: NOTICE:  extension \"x\" already exists, skipping");
        errors.observe("psql:<stdin>:5: WARNING:  no privileges were granted");
        let verdict = errors.verdict(&statements(), &BTreeSet::new());
        assert_eq!(verdict.failed_total, 2, "{verdict:?}");
        assert_eq!(
            verdict.failed[0].to_string(),
            "line 3: rows.csv: No such file or directory"
        );
        // Line 12 is a GRANT line: an untranslatable error is never excused.
        assert_eq!(verdict.roles, 0);
    }

    #[test]
    fn a_notice_quoting_an_error_and_an_included_files_line_are_told_apart() {
        let mut errors = PsqlErrors::default();
        errors.observe("psql:<stdin>:5: NOTICE:  table \"t\" does not exist, skipping: ERROR: no");
        // `\i extra.sql` failing at its own line 12: dump line 12 is a GRANT,
        // which must not excuse it.
        errors.observe("psql:extra.sql:12: ERROR:  role \"app\" does not exist");
        let verdict = errors.verdict(&statements(), &BTreeSet::new());
        assert_eq!(verdict.roles, 0);
        assert_eq!(verdict.failed_total, 1, "{verdict:?}");
        assert_eq!(verdict.failed[0].line, None);
    }

    #[test]
    fn warnings_past_the_kept_messages_still_count_as_warnings() {
        let count = super::MAX_KEPT + 5;
        let mut lines = StatementLines::default();
        for row in 1..=count as u64 {
            lines.role_statements.insert(row);
        }
        let lines = texts(lines);
        // One past the kept messages is on a policy, not an owner line.
        let policy = count as u64 + 1;
        let errors = observed(
            &lines,
            (1..=policy)
                .map(|row| format!("psql:<stdin>:{row}: ERROR:  role \"prod_app\" does not exist")),
        );
        let verdict = errors.verdict(&lines, &BTreeSet::new());
        assert_eq!(verdict.roles, count);
        assert_eq!(verdict.failed_total, 1, "{verdict:?}");
        assert_eq!(
            verdict.failed[0].to_string(),
            format!("line {policy}: a role or owner error")
        );
    }

    #[test]
    fn only_the_first_failures_are_kept_but_all_are_counted() {
        let mut errors = PsqlErrors::default();
        for row in 0..100_000 {
            errors.observe(&format!("psql:<stdin>:{row}: ERROR:  duplicate key value"));
        }
        let verdict = errors.verdict(&StatementLines::default(), &BTreeSet::new());
        assert_eq!(verdict.failed.len(), super::SHOWN);
        assert_eq!(verdict.failed_total, 100_000);
    }

    #[test]
    fn a_warning_needs_psqls_echo_of_its_line_and_only_that() {
        let lines = statements();
        let error = "psql:<stdin>:12: ERROR:  role \"prod_app\" does not exist";
        let verdict_of = |input: &[&str]| {
            let mut errors = PsqlErrors::default();
            input.iter().for_each(|line| errors.observe(line));
            errors.verdict(&lines, &BTreeSet::new())
        };
        // The line's own statement, echoed alone: a warning.
        assert_eq!(
            verdict_of(&[error, "psql:<stdin>:12: STATEMENT:  statement on line 12;"]).roles,
            1
        );
        for input in [
            // No echo (a psql that does not echo, or a lost line).
            &[error][..],
            // Another statement ended on that line (a literal or body the
            // line tap misread).
            &[
                error,
                "psql:<stdin>:12: STATEMENT:  CREATE SCHEMA s AUTHORIZATION missing",
            ],
            // The line's text, but as the start of a longer statement.
            &[
                error,
                "psql:<stdin>:12: STATEMENT:  statement on line 12;",
                "SELECT 1/0;",
            ],
            // A second echo for the line is part of the first's statement
            // (a line of it that reads like psql's).
            &[
                error,
                "psql:<stdin>:12: STATEMENT:  statement on line 12;",
                "psql:<stdin>:12: STATEMENT:  x '",
            ],
            // An echo for another line.
            &[error, "psql:<stdin>:13: STATEMENT:  statement on line 12;"],
        ] {
            let verdict = verdict_of(input);
            assert_eq!((verdict.roles, verdict.failed_total), (0, 1), "{input:?}");
        }
    }

    #[test]
    fn a_dump_and_its_psql_errors_judged_end_to_end() {
        // With standard strings off, psql does not end `'it\'` there, so
        // line 3 ends a CREATE SCHEMA, not a GRANT. The line tap, which
        // cannot tell which reading psql uses there, excuses nothing past
        // it, and psql's echo shows the statement that really failed.
        let dump = "SET standard_conforming_strings = off;\n\
                    CREATE SCHEMA s AUTHORIZATION missing CREATE VIEW v AS SELECT 'it\\';\n\
                    GRANT SELECT ON t TO app; -- ' AS c;\n";
        let (mut tap, lines) =
            super::super::statement_lines::LineTap::new(Box::new(std::io::Cursor::new(dump)), true);
        std::io::copy(&mut tap, &mut std::io::sink()).expect("read");
        let lines = lines.lock().expect("lines");
        assert!(!lines.role_statements.contains(&3), "the tap gave up");
        let mut errors = PsqlErrors::default();
        for line in [
            "psql:<stdin>:3: ERROR:  role \"missing\" does not exist",
            "psql:<stdin>:3: STATEMENT:  CREATE SCHEMA s AUTHORIZATION missing CREATE VIEW v AS SELECT 'it\\';",
            "GRANT SELECT ON t TO app; -- ' AS c;",
        ] {
            errors.observe(line);
        }
        let verdict = errors.verdict(&lines, &BTreeSet::new());
        assert_eq!((verdict.roles, verdict.failed_total), (0, 1), "{verdict:?}");
    }

    #[test]
    fn a_missing_session_authorization_role_is_a_warning_only_for_its_own_line() {
        // pg_dump --use-set-session-authorization names each owner so.
        let dump = "SET SESSION AUTHORIZATION 'prod_app';\nCREATE TABLE t (id int);\n";
        let (mut tap, lines) =
            super::super::statement_lines::LineTap::new(Box::new(std::io::Cursor::new(dump)), true);
        std::io::copy(&mut tap, &mut std::io::sink()).expect("read");
        let lines = lines.lock().expect("lines");
        let verdict_of = |echo: &str| {
            let mut errors = PsqlErrors::default();
            for line in [
                "psql:<stdin>:1: ERROR:  role \"prod_app\" does not exist",
                echo,
            ] {
                errors.observe(line);
            }
            errors.verdict(&lines, &BTreeSet::new())
        };
        let warned =
            verdict_of("psql:<stdin>:1: STATEMENT:  SET SESSION AUTHORIZATION 'prod_app';");
        assert_eq!((warned.roles, warned.failed_total), (1, 0), "{warned:?}");
        let other = verdict_of("psql:<stdin>:1: STATEMENT:  SET SESSION AUTHORIZATION 'other';");
        assert_eq!((other.roles, other.failed_total), (0, 1), "{other:?}");
    }

    #[test]
    fn psqls_echoes_are_read_as_psql_18_prints_them() {
        // From psql 18.6: a refused COPY is echoed on its `\.`, past the
        // data it skipped; an error's `LINE` context comes before its echo.
        let lines = StatementLines {
            rowless_writes: BTreeSet::from([1, 7]),
            copy_tables: std::collections::BTreeMap::from([(1, "public.t".to_owned())]),
            copy_ends: std::collections::BTreeMap::from([(1, 2)]),
            texts: std::collections::BTreeMap::from([
                (1, text_key("COPY public.t (a) FROM stdin;")),
                (7, text_key("SELECT pg_catalog.setval('s', 1, false);")),
            ]),
            ..StatementLines::default()
        };
        let mut errors = PsqlErrors::default();
        for line in [
            "psql:<stdin>:1: ERROR:  permission denied for table t",
            "psql:<stdin>:2: STATEMENT:  COPY public.t (a) FROM stdin;",
            "psql:<stdin>:7: ERROR:  permission denied for sequence s",
            "LINE 1: SELECT pg_catalog.setval('s', 1, false);",
            "                                 ^",
            "psql:<stdin>:7: STATEMENT:  SELECT pg_catalog.setval('s', 1, false);",
        ] {
            errors.observe(line);
        }
        let verdict = errors.verdict(
            &lines,
            &BTreeSet::from(["public.t".to_owned(), "s".to_owned()]),
        );
        assert_eq!(
            (verdict.extension_tables, verdict.failed_total),
            (2, 0),
            "{verdict:?}"
        );
        // A COPY echoed anywhere but its own `\.` is another statement.
        let mut errors = PsqlErrors::default();
        errors.observe("psql:<stdin>:1: ERROR:  permission denied for table t");
        errors.observe("psql:<stdin>:3: STATEMENT:  COPY public.t (a) FROM stdin;");
        let verdict = errors.verdict(&lines, &BTreeSet::from(["public.t".to_owned()]));
        assert_eq!(verdict.failed_total, 1, "{verdict:?}");
    }

    #[test]
    fn psql_13_and_14_echo_a_refused_copy_on_its_own_line() {
        // psql 14 (and 13, and 14's fixed builds) echoes before it skips the
        // data; 13 then runs an empty COPY's `\.` as a command.
        let lines = texts(StatementLines {
            rowless_writes: BTreeSet::from([68]),
            copy_tables: std::collections::BTreeMap::from([(
                68,
                "_timescaledb_catalog.hypertable".to_owned(),
            )]),
            copy_ends: std::collections::BTreeMap::from([(68, 69)]),
            ..StatementLines::default()
        });
        let extension = BTreeSet::from(["_timescaledb_catalog.hypertable".to_owned()]);
        for input in [
            &[
                "psql:<stdin>:68: ERROR:  permission denied for table hypertable",
                "psql:<stdin>:68: STATEMENT:  statement on line 68;",
            ][..],
            &[
                "psql:<stdin>:68: ERROR:  permission denied for table hypertable",
                "psql:<stdin>:68: STATEMENT:  statement on line 68;",
                "psql:<stdin>:69: error: invalid command \\.",
            ][..],
        ] {
            let mut errors = PsqlErrors::default();
            input.iter().for_each(|line| errors.observe(line));
            let verdict = errors.verdict(&lines, &extension);
            assert_eq!(
                (verdict.extension_tables, verdict.failed_total),
                (1, 0),
                "{input:?} {verdict:?}"
            );
        }
    }

    #[test]
    fn an_errors_own_text_cannot_stand_in_for_psqls_echo() {
        // A raised error whose text holds lines that read like psql's: its
        // real echo follows, so two echoes for one error excuse nothing.
        for input in [
            &[
                "psql:<stdin>:12: ERROR:  role \"x\" does not exist",
                "psql:<stdin>:12: DETAIL:  see",
                "psql:<stdin>:12: STATEMENT:  statement on line 12;",
                "psql:<stdin>:1: NOTICE:  x",
                "psql:<stdin>:12: STATEMENT:  DO $$ BEGIN RAISE EXCEPTION 'x'; END $$;",
            ][..],
            &[
                "psql:<stdin>:68: ERROR:  permission denied for table hypertable",
                "psql:<stdin>:69: STATEMENT:  statement on line 68;",
                "psql:<stdin>:68: STATEMENT:  DO $$ BEGIN RAISE EXCEPTION 'x'; END $$;",
            ][..],
            // A line of psql's own in between (tolerated, or judged later)
            // does not end the error's window.
            &[
                "psql:<stdin>:12: ERROR:  role \"x\" does not exist",
                "psql:<stdin>:12: STATEMENT:  statement on line 12;",
                "psql:<stdin>:1: error: invalid command \\restrict",
                "psql:<stdin>:12: STATEMENT:  DO $$ BEGIN RAISE EXCEPTION 'x'; END $$;",
            ][..],
            &[
                "psql:<stdin>:68: ERROR:  permission denied for table hypertable",
                "psql:<stdin>:69: STATEMENT:  statement on line 68;",
                "psql:<stdin>:69: error: invalid command \\.",
                "psql:<stdin>:68: STATEMENT:  DO $$ BEGIN RAISE EXCEPTION 'x'; END $$;",
            ][..],
        ] {
            let mut errors = PsqlErrors::default();
            input.iter().for_each(|line| errors.observe(line));
            let lines = texts(StatementLines {
                role_statements: BTreeSet::from([12]),
                rowless_writes: BTreeSet::from([68]),
                copy_tables: std::collections::BTreeMap::from([(
                    68,
                    "public.hypertable".to_owned(),
                )]),
                copy_ends: std::collections::BTreeMap::from([(68, 69)]),
                ..StatementLines::default()
            });
            let verdict = errors.verdict(&lines, &BTreeSet::from(["public.hypertable".to_owned()]));
            // Nothing excused; the error fails (a stray `\.` fails too).
            assert_eq!(
                (
                    verdict.roles,
                    verdict.extension_tables,
                    verdict.failed_total >= 1
                ),
                (0, 0, true),
                "{input:?}"
            );
        }
    }
}
