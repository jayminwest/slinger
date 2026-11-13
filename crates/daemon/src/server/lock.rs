//! The lock that says which daemon owns a socket path: the lock, not the
//! socket file, since a daemon that died leaves its socket file but no lock.

use std::fs::{File, TryLockError};
use std::io;
use std::path::Path;

/// The lock on `<socket>.lock`, held while this lives.
#[derive(Debug)]
pub(super) struct Lock(File);

impl Lock {
    /// Takes the lock on the path `socket`, or `None` when another holds it.
    pub(super) fn take(socket: &Path) -> io::Result<Option<Self>> {
        let mut path = socket.as_os_str().to_owned();
        path.push(".lock");
        let file = File::options().create(true).append(true).open(path)?;
        match file.try_lock() {
            Ok(()) => Ok(Some(Self(file))),
            Err(TryLockError::WouldBlock) => Ok(None),
            Err(TryLockError::Error(e)) => Err(e),
        }
    }
}

impl Drop for Lock {
    /// Lets go of the path. Closing the file alone is not enough: a process
    /// spawned from another thread holds a copy of it from the fork until
    /// its program starts, and the lock lasts while any copy is open
    /// (slinger-33afa8, as slinger-3229b9 found for the app's unsent ink).
    /// Unlocking frees it for every copy at once.
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
        let socket = dir.path().join("d.sock");
        let held = Lock::take(&socket).unwrap().unwrap();
        assert!(Lock::take(&socket).unwrap().is_none());
        drop(held);
        assert!(Lock::take(&socket).unwrap().is_some());
    }

    /// A spawn's copy of the lock's file, open from its fork to its exec,
    /// does not keep the path from the next daemon.
    #[test]
    fn letting_go_frees_the_path_while_a_spawn_still_holds_a_copy_of_the_lock() {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("d.sock");
        let held = Lock::take(&socket).unwrap().unwrap();
        let spawning = held.0.try_clone().unwrap();
        drop(held);
        assert!(Lock::take(&socket).unwrap().is_some());
        drop(spawning);
    }
}
