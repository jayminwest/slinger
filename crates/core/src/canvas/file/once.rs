//! A request applied once (slinger-1d05ae): a caller that cannot tell
//! whether its `canvas_apply` was written (the answer was lost, or it quit
//! before it came) sends it again under the same key, and the second is
//! not written twice. The log keeps the keys of its last [`ONCES`] such
//! changes, and what each wrote, in memory: a restart of the daemon
//! forgets them, as it forgets the undo history.

use std::collections::VecDeque;

use serde_json::Value;

use super::{Log, Written};
use crate::Result;
use crate::canvas::{Author, Time};

/// How many keys a canvas keeps.
pub const ONCES: usize = 1024;

/// The keys of the last changes applied under one, newest last, each with
/// the numbers of its lines and the ids it made.
#[derive(Debug, Clone, Default)]
pub(super) struct Onces(VecDeque<(String, Vec<u64>, Vec<String>)>);

impl Log {
    /// Applies `ops` as [`Log::apply`] does, under `once`, a key the caller
    /// makes unique (a random one). Under a key one of the last [`ONCES`]
    /// changes was applied under, it applies nothing and writes nothing, and
    /// says the lines and ids that change wrote, so the caller hears what it
    /// would have heard the first time.
    ///
    /// # Errors
    ///
    /// The errors of [`Log::apply`]. A change that failed keeps no key, so
    /// it may be sent again.
    pub fn apply_once(
        &mut self,
        by: &Author,
        at: Time,
        ops: &Value,
        once: &str,
    ) -> Result<Written> {
        if let Some((_, seq, made)) = self.onces.0.iter().find(|(key, ..)| key == once) {
            return Ok(Written { bytes: String::new(), seq: seq.clone(), made: made.clone() });
        }
        let written = self.apply(by, at, ops)?;
        if self.onces.0.len() == ONCES {
            self.onces.0.pop_front();
        }
        self.onces.0.push_back((once.to_owned(), written.seq.clone(), written.made.clone()));
        Ok(written)
    }
}
