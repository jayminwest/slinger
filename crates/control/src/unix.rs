//! Unix sockets at paths of any length. A socket's address holds at most
//! 103 bytes of path on macOS (104 with its NUL; 107 on Linux), and a
//! scratch directory is often deeper than that. A path that is too long
//! goes through a short one in the temporary directory: a connect through
//! a symbolic link to it, a bind at the short path and a rename to the long
//! one, the socket still listening there. Only a longer path takes the
//! detour.

use std::io;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// What the system says of a path too long for a socket's address.
fn too_long(e: &io::Error) -> bool {
    e.kind() == io::ErrorKind::InvalidInput
}

/// Connects to the socket at `path`, however long.
///
/// # Errors
///
/// No socket listens there, or the link to it cannot be made.
pub fn connect_socket(path: &Path) -> io::Result<UnixStream> {
    match UnixStream::connect(path) {
        Err(e) if too_long(&e) => {
            let link = Short::new()?;
            std::os::unix::fs::symlink(std::path::absolute(path)?, &link.0)?;
            UnixStream::connect(&link.0)
        }
        connected => connected,
    }
}

/// Listens on a new socket at `path`, however long.
///
/// # Errors
///
/// The socket cannot be made there: its directory is missing, or (for a
/// long path) on another volume than the temporary directory.
pub fn bind_socket(path: &Path) -> io::Result<UnixListener> {
    match UnixListener::bind(path) {
        Err(e) if too_long(&e) => {
            let short = Short::new()?;
            let listener = UnixListener::bind(&short.0)?;
            std::fs::rename(&short.0, path)?;
            Ok(listener)
        }
        bound => bound,
    }
}

/// A short path no one else uses, in the temporary directory; whatever is
/// there when it is dropped goes.
struct Short(PathBuf);

impl Short {
    fn new() -> io::Result<Self> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let name =
            format!("slinger-{}-{}.sock", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed));
        let tmp = std::env::temp_dir();
        // A temporary directory deep enough to be too long itself: /tmp.
        let dir =
            if tmp.as_os_str().len() + name.len() < 100 { tmp } else { PathBuf::from("/tmp") };
        let path = dir.join(name);
        // Left by an earlier process of the same id.
        match std::fs::remove_file(&path) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e),
            _ => {}
        }
        Ok(Self(path))
    }
}

impl Drop for Short {
    fn drop(&mut self) {
        // Gone already when a bind's socket moved to its long path.
        let _ = std::fs::remove_file(&self.0);
    }
}

#[cfg(test)]
mod tests {
    use std::io::{BufRead, BufReader, Write};

    use super::*;

    #[test]
    fn a_socket_at_a_path_too_long_for_its_address_binds_and_connects() {
        let dir = tempfile::tempdir().unwrap();
        let deep = dir.path().join("a".repeat(60)).join("b".repeat(60));
        std::fs::create_dir_all(&deep).unwrap();
        let path = deep.join("a.sock");
        assert!(path.as_os_str().len() > 120, "{}", path.display());
        assert!(too_long(&UnixListener::bind(&path).unwrap_err()));

        let listener = bind_socket(&path).unwrap();
        let mut client = connect_socket(&path).unwrap();
        let (server, _) = listener.accept().unwrap();
        writeln!(client, "hi").unwrap();
        let mut line = String::new();
        BufReader::new(&server).read_line(&mut line).unwrap();
        assert_eq!(line, "hi\n");
        // The socket is at the long path, and no link or short socket is left.
        assert!(path.exists());
        let left = std::fs::read_dir(std::env::temp_dir()).unwrap().flatten().any(|e| {
            e.file_name().to_string_lossy().starts_with(&format!("slinger-{}-", std::process::id()))
        });
        assert!(!left, "a short path is left behind");
    }

    #[test]
    fn a_short_path_is_used_as_it_is() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.sock");
        let _listener = bind_socket(&path).unwrap();
        connect_socket(&path).unwrap();
        let missing = connect_socket(&dir.path().join("none.sock")).unwrap_err();
        assert_eq!(missing.kind(), io::ErrorKind::NotFound);
    }
}
