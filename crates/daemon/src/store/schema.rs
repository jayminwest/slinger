//! The record file's layout: its tables, its version, and the steps that
//! bring an older file up to date.
//!
//! The file keeps the machine name it was created under, so a host rename
//! does not split one record between two names (D15).

use std::path::Path;

use rusqlite::{OptionalExtension, Transaction};

use super::{Result, StoreError};

/// The layout this code writes, kept in SQLite's `user_version`.
pub(super) const VERSION: i64 = 2;

/// Layout 1: the event log and the blobs.
pub(super) const V1: &str = "
CREATE TABLE events (
    number    INTEGER PRIMARY KEY,
    time      INTEGER NOT NULL,
    kind      TEXT NOT NULL,
    machine   TEXT NOT NULL,
    workspace TEXT,
    agent     TEXT,
    canvas    TEXT,
    event     TEXT NOT NULL
) STRICT;
CREATE INDEX events_by_kind      ON events (kind, number);
CREATE INDEX events_by_workspace ON events (workspace, number);
CREATE INDEX events_by_agent     ON events (agent, number);
CREATE INDEX events_by_canvas    ON events (canvas, number);
CREATE TRIGGER events_no_update BEFORE UPDATE ON events
    BEGIN SELECT RAISE(ABORT, 'the record is append-only'); END;
CREATE TRIGGER events_no_delete BEFORE DELETE ON events
    BEGIN SELECT RAISE(ABORT, 'the record is append-only'); END;
CREATE TABLE blobs (
    id    TEXT PRIMARY KEY,
    bytes BLOB NOT NULL
) STRICT, WITHOUT ROWID;
CREATE TRIGGER blobs_no_update BEFORE UPDATE ON blobs
    BEGIN SELECT RAISE(ABORT, 'a blob never changes'); END;
";

/// Layout 2: the file's own settings, written once; `machine` is the name
/// every event is tagged with.
const V2: &str = "
CREATE TABLE settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
) STRICT, WITHOUT ROWID;
";

/// Lays out the file at `path` or brings it up to [`VERSION`], inside `tx`,
/// and returns the machine name the record keeps. A new record keeps
/// `machine`; a layout-1 record keeps the name on its first event, else
/// `machine`.
///
/// # Errors
///
/// [`StoreError::Newer`] when a newer slingerd laid the file out, and
/// [`StoreError::Sqlite`] when a read or write fails.
pub(super) fn prepare(tx: &Transaction<'_>, path: &Path, machine: String) -> Result<String> {
    let found: i64 = tx.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if found > VERSION {
        return Err(StoreError::Newer { path: path.to_owned(), found });
    }
    if found < 1 {
        tx.execute_batch(V1)?;
    }
    if found < 2 {
        tx.execute_batch(V2)?;
        let first: Option<String> = tx
            .query_row("SELECT machine FROM events ORDER BY number LIMIT 1", [], |row| row.get(0))
            .optional()?;
        tx.execute(
            "INSERT INTO settings (key, value) VALUES ('machine', ?1)",
            [first.unwrap_or(machine)],
        )?;
        tx.pragma_update(None, "user_version", VERSION)?;
    }
    Ok(tx.query_row("SELECT value FROM settings WHERE key = 'machine'", [], |row| row.get(0))?)
}
