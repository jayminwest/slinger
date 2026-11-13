//! The slings that wait for a listener, as the record keeps them (layout 3):
//! a row from the sling's `sling_sent` until its listener deals with it, so
//! a daemon that stops and starts again loses none.
//!
//! [`crate::listeners::Listeners`] decides what waits; this is its copy on
//! disk, sharing the record's connection.

use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use rusqlite::{Connection, params};
use slinger_core::session::listen::Heard;

use super::Result;

/// The record's table of waiting slings. Safe to share between threads.
#[derive(Debug, Clone)]
pub struct Waiting {
    db: Arc<Mutex<Connection>>,
}

impl Waiting {
    pub(super) fn new(db: Arc<Mutex<Connection>>) -> Self {
        Self { db }
    }

    /// Keeps `sling`, which waits for `agent`'s listener. Keeping it again
    /// changes nothing.
    ///
    /// # Errors
    ///
    /// [`super::StoreError::Sqlite`] when the write fails.
    pub fn keep(&self, agent: &str, sling: &Heard) -> Result<()> {
        self.db().execute(
            "INSERT INTO waiting (event, agent, canvas, dir, text) VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT (event) DO NOTHING",
            params![
                super::to_i64(sling.event),
                agent,
                sling.canvas,
                sling.dir.as_os_str().as_bytes(),
                sling.text
            ],
        )?;
        Ok(())
    }

    /// Forgets the sling of the `sling_sent` numbered `event`: its listener
    /// dealt with it. One that is not kept is no error.
    ///
    /// # Errors
    ///
    /// [`super::StoreError::Sqlite`] when the write fails.
    pub fn forget(&self, event: u64) -> Result<()> {
        self.db().execute("DELETE FROM waiting WHERE event = ?1", [super::to_i64(event)])?;
        Ok(())
    }

    /// Every sling kept, with its agent, oldest first.
    ///
    /// # Errors
    ///
    /// [`super::StoreError::Sqlite`] when the read fails.
    pub fn all(&self) -> Result<Vec<(String, Heard)>> {
        let db = self.db();
        let mut statement =
            db.prepare("SELECT event, agent, canvas, dir, text FROM waiting ORDER BY event")?;
        let rows = statement.query_map([], |row| {
            let dir: Vec<u8> = row.get(3)?;
            let sling = Heard {
                event: super::to_u64(row.get(0)?),
                canvas: row.get(2)?,
                dir: PathBuf::from(OsStr::from_bytes(&dir)),
                text: row.get(4)?,
            };
            Ok((row.get(1)?, sling))
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    fn db(&self) -> MutexGuard<'_, Connection> {
        self.db.lock().unwrap_or_else(PoisonError::into_inner)
    }
}
