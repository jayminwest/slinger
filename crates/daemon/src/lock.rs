//! The locks that say who owns a place: the socket's `.lock`, which says
//! which daemon serves the path, and the data directory's `record.lock`,
//! which says which store owns the record. A lock, not the file it guards,
//! since a process that died leaves its files but no lock.

use std::fs::{File, TryLockError};
use std::io;
use std::path::Path;

/// The lock on a file, held while this lives.
#[derive(Debug)]
pub(crate) struct Lock(File);

impl Lock {
    /// Takes the lock on the file at `path`, creating it, or `None` when
    /// another holds it.
    pub(crate) fn take(path: &Path) -> io::Result<Option<Self>> {
        let file = File::options().create(true).append(true).open(path)?;
        match file.try_lock() {
            Ok(()) => Ok(Some(Self(file))),
            Err(TryLockError::WouldBlock) => Ok(None),
            Err(TryLockError::Error(e)) => Err(e),
        }
    }
}

impl Drop for Lock {
    /// Lets go of the file. Closing it alone is not enough: a process
    /// spawned from another thread holds a copy of it from the fork until
    /// its program starts, and the lock lasts while any copy is open
    /// (slinger-33afa8 and slinger-2e08c8, as slinger-3229b9 found for the
    /// app's unsent ink). Unlocking frees it for every copy at once.
    fn drop(&mut self) {
        // Closed next, which lets go too, but for this copy alone.
        let _ = self.0.unlock();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_holder_at_a_time() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("d.lock");
        let held = Lock::take(&path).unwrap().unwrap();
        assert!(Lock::take(&path).unwrap().is_none());
        drop(held);
        assert!(Lock::take(&path).unwrap().is_some());
    }

    /// A spawn's copy of the lock's file, open from its fork to its exec,
    /// does not keep the lock from the next taker.
    #[test]
    fn letting_go_frees_the_lock_while_a_spawn_still_holds_a_copy_of_its_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("d.lock");
        let held = Lock::take(&path).unwrap().unwrap();
        let spawning = held.0.try_clone().unwrap();
        drop(held);
        assert!(Lock::take(&path).unwrap().is_some());
        drop(spawning);
    }
}
