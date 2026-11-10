//! The record on disk (SPEC section 9, D14, D15): one SQLite file in the data
//! directory that holds this machine's event log and its blobs.
//!
//! This module owns the storage layout. Nothing else knows the file's name,
//! its tables or its SQL. An event is stored as the line
//! [`record::encode`] writes, byte for byte, so fields this daemon does not
//! know survive; the other columns only index it.
//!
//! The log is append-only: the store assigns each event its number and time
//! and stamps its machine, and the file refuses updates and deletes.

#[cfg(test)]
mod tests;

use std::fs::DirBuilder;
use std::os::unix::fs::DirBuilderExt;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{
    Connection, OptionalExtension, Transaction, TransactionBehavior, params, params_from_iter,
};
use slinger_core::record::{self, BlobId, Event};

/// The store's file inside the data directory.
const FILE: &str = "record.sqlite";
/// The layout this code writes, kept in SQLite's `user_version`.
const SCHEMA_VERSION: i64 = 1;
const SCHEMA: &str = "
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

/// Why the store failed.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    /// No data directory was given and none could be found.
    #[error("cannot find a data directory")]
    NoDataDir,
    #[error("cannot create data directory {}: {source}", path.display())]
    Dir { path: PathBuf, source: std::io::Error },
    #[error("cannot open the record {}: {source}", path.display())]
    Open { path: PathBuf, source: rusqlite::Error },
    /// The file was laid out by a newer slingerd.
    #[error("the record {} has layout {found}, newer than this slingerd reads ({SCHEMA_VERSION})", path.display())]
    Newer { path: PathBuf, found: i64 },
    #[error("record database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    /// A stored event does not decode. The writer broke the format (B5).
    #[error("stored event {number} is unreadable: {source}")]
    Unreadable { number: u64, source: slinger_core::Error },
}

impl StoreError {
    /// Stable machine-readable code.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::NoDataDir => "invalid",
            Self::Newer { .. } => "conflict",
            Self::Dir { .. } | Self::Open { .. } | Self::Sqlite(_) | Self::Unreadable { .. } => {
                "internal"
            }
        }
    }

    /// What the caller can do about it, if anything.
    #[must_use]
    pub fn hint(&self) -> Option<String> {
        match self {
            Self::NoDataDir => Some("set HOME, or pass --data-dir".into()),
            Self::Dir { .. } | Self::Open { .. } => Some("pass another --data-dir".into()),
            Self::Newer { .. } => Some("run a newer slingerd, or pass another --data-dir".into()),
            Self::Sqlite(_) | Self::Unreadable { .. } => None,
        }
    }
}

type Result<T> = std::result::Result<T, StoreError>;

/// Which events to read. Every field narrows; the default reads them all.
/// A range or filter that matches nothing is an empty result, not an error.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Query {
    /// The first event number, inclusive.
    pub from: Option<u64>,
    /// The last event number, inclusive.
    pub to: Option<u64>,
    /// Only this kind.
    pub kind: Option<String>,
    /// Only events with this machine tag.
    pub machine: Option<String>,
    /// Only events with this workspace tag.
    pub workspace: Option<String>,
    /// Only events with this agent tag.
    pub agent: Option<String>,
    /// Only events with this canvas tag.
    pub canvas: Option<String>,
    /// At most this many, the lowest numbers first.
    pub limit: Option<u64>,
}

/// One machine's record. Safe to share between threads: calls take turns.
#[derive(Debug)]
pub struct Store {
    db: Mutex<Connection>,
    machine: String,
}

impl Store {
    /// Opens the record in `dir`, creating the directory (mode 0700) and the
    /// file when they do not exist. Every event appended is tagged `machine`.
    ///
    /// # Errors
    ///
    /// [`StoreError::Dir`] or [`StoreError::Open`] when the directory or the
    /// file cannot be made or read, and [`StoreError::Newer`] when a newer
    /// slingerd laid the file out.
    pub fn open(dir: &Path, machine: impl Into<String>) -> Result<Self> {
        DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(dir)
            .map_err(|source| StoreError::Dir { path: dir.to_owned(), source })?;
        let path = dir.join(FILE);
        let opened = |source| StoreError::Open { path: path.clone(), source };
        let mut db = Connection::open(&path).map_err(opened)?;
        db.pragma_update(None, "journal_mode", "WAL").map_err(opened)?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate).map_err(opened)?;
        let found: i64 = tx.pragma_query_value(None, "user_version", |row| row.get(0))?;
        match found {
            0 => {
                tx.execute_batch(SCHEMA)?;
                tx.pragma_update(None, "user_version", SCHEMA_VERSION)?;
            }
            SCHEMA_VERSION => {}
            _ => return Err(StoreError::Newer { path, found }),
        }
        tx.commit()?;
        Ok(Self { db: Mutex::new(db), machine: machine.into() })
    }

    /// Appends `event` to the log and returns it as stored: with the next
    /// number, the current time and this store's machine tag, whatever the
    /// caller set them to.
    ///
    /// # Errors
    ///
    /// [`StoreError::Sqlite`] when the write fails.
    pub fn append(&self, event: Event) -> Result<Event> {
        let mut db = self.db();
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let event = self.insert(&tx, event)?;
        tx.commit()?;
        Ok(event)
    }

    /// Reads the events `query` selects and gives them to `decide`. When it
    /// returns `Ok(event)`, appends that event as [`Store::append`] does and
    /// returns it as stored; `Err(t)` appends nothing and comes back as it is.
    /// No other append lands between the read and the write.
    ///
    /// # Errors
    ///
    /// [`StoreError::Sqlite`] when the read or the write fails, and
    /// [`StoreError::Unreadable`] when a stored event does not decode.
    pub fn append_if<T>(
        &self,
        query: &Query,
        decide: impl FnOnce(&[Event]) -> std::result::Result<Event, T>,
    ) -> Result<std::result::Result<Event, T>> {
        let mut db = self.db();
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let event = match decide(&select(&tx, query)?) {
            Ok(event) => event,
            Err(t) => return Ok(Err(t)),
        };
        let event = self.insert(&tx, event)?;
        tx.commit()?;
        Ok(Ok(event))
    }

    /// The events `query` selects, in log order.
    ///
    /// # Errors
    ///
    /// [`StoreError::Sqlite`] when the read fails, and
    /// [`StoreError::Unreadable`] when a stored event does not decode.
    pub fn events(&self, query: &Query) -> Result<Vec<Event>> {
        select(&self.db(), query)
    }

    /// Stores `bytes` and returns their id. Putting the same bytes again
    /// stores nothing new and returns the same id.
    ///
    /// # Errors
    ///
    /// [`StoreError::Sqlite`] when the write fails.
    pub fn put_blob(&self, bytes: &[u8]) -> Result<BlobId> {
        let id = BlobId::of(bytes);
        self.db().execute(
            "INSERT INTO blobs (id, bytes) VALUES (?1, ?2) ON CONFLICT (id) DO NOTHING",
            params![id.to_string(), bytes],
        )?;
        Ok(id)
    }

    /// The bytes of the blob `id`, or `None` when this record does not hold it.
    ///
    /// # Errors
    ///
    /// [`StoreError::Sqlite`] when the read fails.
    pub fn blob(&self, id: BlobId) -> Result<Option<Vec<u8>>> {
        Ok(self
            .db()
            .query_row("SELECT bytes FROM blobs WHERE id = ?1", [id.to_string()], |row| row.get(0))
            .optional()?)
    }

    /// Gives `event` the next number, the time and this machine, and writes it.
    fn insert(&self, tx: &Transaction<'_>, mut event: Event) -> Result<Event> {
        let last: i64 =
            tx.query_row("SELECT COALESCE(MAX(number), 0) FROM events", [], |row| row.get(0))?;
        let number = last + 1;
        event.number = Some(to_u64(number));
        event.time = now_ms();
        event.tags.machine.clone_from(&self.machine);
        tx.execute(
            "INSERT INTO events (number, time, kind, machine, workspace, agent, canvas, event)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                number,
                to_i64(event.time),
                event.body.kind(),
                event.tags.machine,
                event.tags.workspace,
                event.tags.agent,
                event.tags.canvas,
                record::encode(&event),
            ],
        )?;
        Ok(event)
    }

    /// The connection. A panic in another call cannot leave it mid-write:
    /// an unfinished transaction rolls back when it is dropped.
    fn db(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.db.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// The events `query` selects on `db`, in log order.
fn select(db: &Connection, query: &Query) -> Result<Vec<Event>> {
    use rusqlite::types::Value;
    let mut args = vec![
        Value::Integer(query.from.map_or(0, to_i64)),
        Value::Integer(query.to.map_or(i64::MAX, to_i64)),
    ];
    let mut conditions = vec!["number >= ?1".to_owned(), "number <= ?2".to_owned()];
    for (column, value) in [
        ("kind", &query.kind),
        ("machine", &query.machine),
        ("workspace", &query.workspace),
        ("agent", &query.agent),
        ("canvas", &query.canvas),
    ] {
        if let Some(value) = value {
            args.push(Value::Text(value.clone()));
            conditions.push(format!("{column} = ?{}", args.len()));
        }
    }
    args.push(Value::Integer(query.limit.map_or(-1, to_i64)));
    let sql = format!(
        "SELECT number, event FROM events WHERE {} ORDER BY number LIMIT ?{}",
        conditions.join(" AND "),
        args.len()
    );
    let mut statement = db.prepare(&sql)?;
    let rows = statement.query_map(params_from_iter(args), |row| {
        Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
    })?;
    rows.map(|row| {
        let (number, json) = row?;
        record::decode(&json)
            .map_err(|source| StoreError::Unreadable { number: to_u64(number), source })
    })
    .collect()
}

/// Where the record lives when `--data-dir` is not given:
/// `$XDG_DATA_HOME/slinger`, else `~/Library/Application Support/slinger` on
/// macOS and `~/.local/share/slinger` elsewhere.
///
/// # Errors
///
/// [`StoreError::NoDataDir`] when neither `XDG_DATA_HOME` nor `HOME` is set.
pub fn default_data_dir() -> Result<PathBuf> {
    data_dir_in(
        std::env::var_os("XDG_DATA_HOME").map(PathBuf::from),
        std::env::home_dir(),
        cfg!(target_os = "macos"),
    )
    .ok_or(StoreError::NoDataDir)
}

fn data_dir_in(xdg: Option<PathBuf>, home: Option<PathBuf>, macos: bool) -> Option<PathBuf> {
    let nonempty = |dir: &PathBuf| !dir.as_os_str().is_empty();
    if let Some(xdg) = xdg.filter(nonempty) {
        return Some(xdg.join("slinger"));
    }
    let home = home.filter(nonempty)?;
    let base =
        if macos { home.join("Library/Application Support") } else { home.join(".local/share") };
    Some(base.join("slinger"))
}

/// This machine's name for the `machine` tag: the host name up to its first
/// dot, so `my-laptop.local` and `my-laptop` are the same machine.
#[must_use]
pub fn machine_name() -> String {
    machine_from(&gethostname::gethostname().to_string_lossy())
}

fn machine_from(host: &str) -> String {
    match host.split('.').next() {
        Some(name) if !name.is_empty() => name.to_owned(),
        _ => "localhost".to_owned(),
    }
}

fn now_ms() -> u64 {
    // A clock before 1970 is a broken clock; 0 keeps the event rather than losing it.
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

/// SQLite integers are signed. Numbers and times never reach `i64::MAX`;
/// a filter past it means "no bound".
fn to_i64(n: u64) -> i64 {
    i64::try_from(n).unwrap_or(i64::MAX)
}

/// Stored numbers are never negative: the store writes them.
fn to_u64(n: i64) -> u64 {
    u64::try_from(n).unwrap_or(0)
}
