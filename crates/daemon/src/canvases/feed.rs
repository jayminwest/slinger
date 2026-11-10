//! A canvas's change feed: the stored lines after a point, read back from the
//! file as written, and a wait for the next change.

use std::os::unix::fs::FileExt;
use std::sync::PoisonError;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use serde_json::Value;
use slinger_core::canvas::file::lines;

use super::{Canvases, Result, failed, lock};

/// The most bytes of lines one reply carries. A longer backlog comes in parts.
const MAX_BYTES: u64 = 1024 * 1024;

impl Canvases {
    /// The operation lines numbered above `after`, lowest first, and the
    /// number after the last one. When there are none, waits up to `wait` for
    /// a change, or until [`Canvases::stop`]; an empty list then succeeds.
    ///
    /// # Errors
    ///
    /// `not_found` for a canvas that is not there, `invalid` for an `after`
    /// the canvas has not reached, and the errors of reading the file.
    pub fn changes(&self, id: &str, after: u64, wait: Duration) -> Result<(Vec<Value>, u64)> {
        let deadline = Instant::now() + wait;
        let slot = self.slot(id)?;
        let mut guard = lock(&slot);
        loop {
            let open = self.load(id, &mut guard)?;
            let span = open.log.after(after, MAX_BYTES)?;
            let left = deadline.saturating_duration_since(Instant::now());
            if !span.bytes.is_empty() || left.is_zero() || self.stopping.load(Ordering::SeqCst) {
                let len = usize::try_from(span.bytes.end - span.bytes.start).unwrap_or(usize::MAX);
                let mut bytes = vec![0; len];
                let path = self.path(id);
                open.file
                    .read_exact_at(&mut bytes, span.bytes.start)
                    .map_err(failed("read", &path))?;
                return Ok((lines(&bytes)?, span.next));
            }
            guard =
                slot.changed.wait_timeout(guard, left).unwrap_or_else(PoisonError::into_inner).0;
        }
    }
}
