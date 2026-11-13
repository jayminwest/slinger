//! Where the daemon is (docs/service.md, "Socket"). A daemon may listen on
//! any path, and the default path follows `$TMPDIR`, which differs between a
//! login shell and the programs an agent runs. So the daemon that serves the
//! default data directory names its socket there, in one file, and a client
//! that is given no socket takes that one: it finds the daemon wherever it
//! listens (slinger-3bd4a6). A name left by a daemon that died names a
//! socket nothing listens on: a client passes over it (slinger-a5678b).

use std::fs;
use std::io;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};

const SOCKET_FILE: &str = "slinger.sock";

/// The file in a data directory that names the socket of the daemon that
/// serves it, while it runs.
const NAMED: &str = "socket";

/// Where the daemon listens when no path is given: `$XDG_RUNTIME_DIR`, else
/// the temporary directory (per user on macOS).
#[must_use]
pub fn default_socket() -> PathBuf {
    socket_in(std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from), std::env::temp_dir())
}

fn socket_in(runtime: Option<PathBuf>, temp: PathBuf) -> PathBuf {
    runtime.filter(|dir| !dir.as_os_str().is_empty()).unwrap_or(temp).join(SOCKET_FILE)
}

/// The live socket the daemon of the default data directory named: where a
/// client given no socket finds the daemon, before it tries
/// [`default_socket`].
#[must_use]
pub fn running_socket() -> Option<PathBuf> {
    default_data_dir().and_then(|dir| live_socket(&dir))
}

/// The socket the daemon serving `data_dir` named, while something listens
/// on it. A daemon that died uncleanly leaves its name and its socket file,
/// which refuses connections: that name is passed over, so a client never
/// holds on a dead socket. The check is one connect-and-close, which the
/// daemon reads as no request and answers nothing.
#[must_use]
pub fn live_socket(data_dir: &Path) -> Option<PathBuf> {
    named_socket(data_dir).filter(|named| UnixStream::connect(named).is_ok())
}

/// Where the record lives when no data directory is given:
/// `$XDG_DATA_HOME/slinger`, else `~/Library/Application Support/slinger` on
/// macOS and `~/.local/share/slinger` elsewhere. `None` when neither
/// `XDG_DATA_HOME` nor `HOME` is set.
#[must_use]
pub fn default_data_dir() -> Option<PathBuf> {
    data_dir_in(
        std::env::var_os("XDG_DATA_HOME").map(PathBuf::from),
        std::env::home_dir(),
        cfg!(target_os = "macos"),
    )
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

/// The file in `data_dir` that names its daemon's socket, there while that
/// daemon serves: a client that follows the daemon watches it.
#[must_use]
pub fn socket_name_file(data_dir: &Path) -> PathBuf {
    data_dir.join(NAMED)
}

/// The socket the daemon serving `data_dir` named, if one did.
#[must_use]
pub fn named_socket(data_dir: &Path) -> Option<PathBuf> {
    let text = fs::read_to_string(socket_name_file(data_dir)).ok()?;
    let path = text.trim_end_matches('\n');
    (!path.is_empty()).then(|| PathBuf::from(path))
}

/// Names `socket` as the one the daemon serving `data_dir` listens on. Only
/// that daemon calls it: it holds the directory's lock.
///
/// # Errors
///
/// The file cannot be written.
pub fn name_socket(data_dir: &Path, socket: &Path) -> io::Result<()> {
    let socket = std::path::absolute(socket)?;
    fs::write(socket_name_file(data_dir), format!("{}\n", socket.display()))
}

/// Takes the name back as the daemon stops, so no client goes to a socket
/// that is gone. A file that is not there is fine.
///
/// # Errors
///
/// The file is there and cannot be removed.
pub fn unname_socket(data_dir: &Path) -> io::Result<()> {
    match fs::remove_file(socket_name_file(data_dir)) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_socket_prefers_the_runtime_dir() {
        let (run, temp) = (PathBuf::from("/run/user/1"), PathBuf::from("/tmp"));
        assert_eq!(socket_in(Some(run.clone()), temp.clone()), run.join(SOCKET_FILE));
        assert_eq!(socket_in(Some("".into()), temp.clone()), temp.join(SOCKET_FILE));
        assert_eq!(socket_in(None, temp.clone()), temp.join(SOCKET_FILE));
        assert!(default_socket().ends_with(SOCKET_FILE));
    }

    #[test]
    fn the_default_data_dir_follows_xdg_then_the_platform() {
        let (xdg, home) = (PathBuf::from("/x"), PathBuf::from("/h"));
        let mac = data_dir_in(Some("".into()), Some(home.clone()), true);
        assert_eq!(
            data_dir_in(Some(xdg.clone()), Some(home.clone()), true),
            Some(xdg.join("slinger"))
        );
        assert_eq!(mac, Some(home.join("Library/Application Support/slinger")));
        let other = data_dir_in(None, Some(home.clone()), false);
        assert_eq!(other, Some(home.join(".local/share/slinger")));
        assert_eq!(data_dir_in(None, Some("".into()), false), None);
        assert_eq!(data_dir_in(None, None, true), None);
    }

    #[test]
    fn a_named_socket_is_found_until_it_is_taken_back() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(named_socket(dir.path()), None);
        name_socket(dir.path(), Path::new("/run/elsewhere.sock")).unwrap();
        assert_eq!(named_socket(dir.path()), Some(PathBuf::from("/run/elsewhere.sock")));
        unname_socket(dir.path()).unwrap();
        assert_eq!(named_socket(dir.path()), None);
        unname_socket(dir.path()).unwrap();
        fs::write(dir.path().join(NAMED), "\n").unwrap();
        assert_eq!(named_socket(dir.path()), None, "an empty name names nothing");
    }

    #[test]
    fn a_name_nothing_listens_on_is_not_live() {
        let (data, sockets) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        assert_eq!(live_socket(data.path()), None, "nothing named");
        let socket = sockets.path().join("d.sock");
        let listening = std::os::unix::net::UnixListener::bind(&socket).unwrap();
        name_socket(data.path(), &socket).unwrap();
        assert_eq!(live_socket(data.path()), Some(socket.clone()));
        // A socket file nothing listens on, as an unclean death leaves it.
        drop(listening);
        assert!(socket.exists());
        assert_eq!(live_socket(data.path()), None);
        assert_eq!(named_socket(data.path()), Some(socket), "the name itself is kept");
    }

    #[test]
    fn a_relative_socket_is_named_by_its_absolute_path() {
        let dir = tempfile::tempdir().unwrap();
        name_socket(dir.path(), Path::new("d.sock")).unwrap();
        let named = named_socket(dir.path()).unwrap();
        assert_eq!(named, std::env::current_dir().unwrap().join("d.sock"));
    }
}
