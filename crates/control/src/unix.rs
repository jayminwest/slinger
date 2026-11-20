//! Unix sockets at paths of any length. A socket's address holds at most
//! 103 bytes of path on macOS (104 with its NUL; 107 on Linux), and a
//! scratch directory is often deeper than that. A path that is too long
//! goes through a short symbolic link in the temporary directory: a connect
//! through a link to the socket, a bind through a link to the socket's
//! directory, so the socket is made where it belongs, on any volume. Only
//! a longer path takes the detour, and the link goes once it is used.

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
            let link = Short::new(0)?;
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
/// The socket cannot be made there: its directory is missing, or its file
/// name alone is too long for a socket's address (`InvalidInput`).
pub fn bind_socket(path: &Path) -> io::Result<UnixListener> {
    match UnixListener::bind(path) {
        Err(e) if too_long(&e) => {
            let (Some(dir), Some(name)) = (path.parent(), path.file_name()) else { return Err(e) };
            let link = Short::new(name.len() + 1)?;
            std::os::unix::fs::symlink(std::path::absolute(dir)?, &link.0)?;
            UnixListener::bind(link.0.join(name)).map_err(|e| {
                if too_long(&e) {
                    let name = name.to_string_lossy();
                    io::Error::new(e.kind(), format!("socket file name {name:?} is too long"))
                } else {
                    e
                }
            })
        }
        bound => bound,
    }
}

/// A short path no one else uses, in the temporary directory; whatever is
/// there when it is dropped goes.
struct Short(PathBuf);

impl Short {
    /// One with room for `more` bytes after it.
    fn new(more: usize) -> io::Result<Self> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let name =
            format!("slinger-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed));
        let tmp = std::env::temp_dir();
        // A temporary directory deep enough to be too long itself: /tmp.
        let dir = if tmp.as_os_str().len() + name.len() + more < 100 {
            tmp
        } else {
            PathBuf::from("/tmp")
        };
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
        // A link: the socket or the directory it names stays.
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
    fn a_long_path_whose_file_name_alone_is_too_long_says_so() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(format!("{}.sock", "n".repeat(110)));
        let e = bind_socket(&path).unwrap_err();
        assert!(too_long(&e), "{e}");
        assert!(e.to_string().contains("file name"), "{e}");
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
