//! psql's arguments for a restore, and the markers berth's own commands
//! around the dump print.

/// What berth's command before the dump prints, then the server process
/// that runs it (`pg_backend_pid()`).
pub(crate) const START_MARKER: &str = "berth-backend-start ";

/// What berth's commands after the dump print: the server process then,
/// whether no transaction is left open (`true`), then psql's
/// `ON_ERROR_ROLLBACK`. A dump that switched connection (`\connect`, even
/// quietly), left a transaction open (psql rolls it back when it exits),
/// turned `ON_ERROR_ROLLBACK` off, or quit (`\q`) shows it here, whatever
/// berth could read of the dump itself.
pub(crate) const END_MARKER: &str = "berth-backend-end ";
pub(crate) const ROLLBACK_MARKER: &str = "berth-rollback ";

/// `psql` and its options before the connection arguments: `-f -` makes
/// each error name its dump line and `-b` (`--echo-errors`) follows it with
/// the failed statement (`database/restore/psql_errors.rs`);
/// `ON_ERROR_ROLLBACK` keeps a skipped owner or grant from aborting an
/// explicit transaction around it (whose COMMIT would then roll everything
/// back silently); `-X` keeps an image's psqlrc out. The `-c` commands run
/// before and after the dump in order, without changing its line numbers;
/// those after it undo what the dump may have set (`\o`, formats, `QUIET`).
/// They run even after a dump's `\q`, which ends only psql's reading of
/// the dump: berth's token after the dump (`database/restore/process.rs`
/// `DumpEnd`) shows that it was read to its end. The end query needs
/// PostgreSQL 10 or later (`txid_current_if_assigned`).
pub(crate) fn psql_restore_args() -> Vec<String> {
    [
        "psql",
        "-X",
        "-b",
        "-v",
        "ON_ERROR_ROLLBACK=on",
        "-c",
        "\\set QUIET on",
        "-c",
        "\\pset tuples_only on",
        "-c",
        // No row on a server that lets a dump hide its errors: before 10.7
        // and 11.2, `SET client_min_messages = fatal` silenced them
        // (PostgreSQL 11.2 / 10.7 release notes).
        "SELECT 'berth-backend-start ' OPERATOR(pg_catalog.||) pg_catalog.pg_backend_pid() \
         WHERE pg_catalog.current_setting('server_version_num')::integer >= 100007 \
         AND pg_catalog.current_setting('server_version_num')::integer NOT BETWEEN 110000 AND 110001",
        "-c",
        "\\pset tuples_only off",
        "-c",
        "\\set QUIET off",
        "-f",
        "-",
        "-c",
        "\\set QUIET on",
        "-c",
        "\\o",
        // Ends a line a dump's `\echo -n` left open.
        "-c",
        "\\echo",
        "-c",
        "\\pset format aligned",
        "-c",
        "\\pset expanded off",
        "-c",
        "\\pset border 1",
        "-c",
        "\\pset tuples_only on",
        "-c",
        // Every name qualified: the dump may have put a schema of its own
        // ahead of `pg_catalog` (`SET search_path`).
        "SELECT 'berth-backend-end ' OPERATOR(pg_catalog.||) pg_catalog.pg_backend_pid() \
         OPERATOR(pg_catalog.||) ' ' OPERATOR(pg_catalog.||) \
         (pg_catalog.txid_current_if_assigned() IS NULL)",
        "-c",
        "\\echo berth-rollback :ON_ERROR_ROLLBACK",
    ]
    .map(ToOwned::to_owned)
    .to_vec()
}
