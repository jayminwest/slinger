//! A socket path longer than a socket's address holds (103 bytes on macOS):
//! the daemon binds it, clients reach it, a stop wakes it and the data
//! directory names it (slinger-45faf4).

use std::path::PathBuf;

use slinger_control::{self as control, Client};
use tempfile::TempDir;

use super::{left_in, open};
use crate::Server;

/// A socket path named `name` in a new directory deep enough to be too
/// long for a socket's address.
fn deep(name: &str) -> (TempDir, PathBuf) {
    let dir = TempDir::new().unwrap();
    let deep = dir.path().join("a".repeat(60)).join("b".repeat(60));
    std::fs::create_dir_all(&deep).unwrap();
    let socket = deep.join(name);
    assert!(socket.as_os_str().len() > 120, "{}", socket.display());
    (dir, socket)
}

#[test]
fn a_daemon_serves_a_socket_path_too_long_for_its_address() {
    let (dir, socket) = deep("d.sock");
    let data = TempDir::new().unwrap();
    let server = Server::bind(&socket).unwrap();
    let stopper = server.stopper();
    let store = open(data.path()).with_claude("/nonexistent/claude");
    let thread = std::thread::spawn(move || server.serve(&store));

    let client = Client::new(&socket);
    assert_eq!(client.daemon_info().unwrap().socket, socket.display().to_string());
    client.canvas_create("notes").unwrap();
    // The name file in the data directory leads to the live socket.
    control::name_socket(data.path(), &socket).unwrap();
    assert_eq!(control::live_socket(data.path()), Some(socket.clone()));

    // The stop reaches the accept loop through the long path too.
    stopper.stop();
    thread.join().unwrap().unwrap();
    assert_eq!(left_in(socket.parent().unwrap()), ["d.sock.lock"]);
    assert_eq!(control::live_socket(data.path()), None);
    drop(dir);
}

#[test]
fn a_socket_file_name_too_long_alone_is_a_bad_argument() {
    let (_dir, socket) = deep(&format!("{}.sock", "n".repeat(110)));
    let e = Server::bind(&socket).unwrap_err();
    assert_eq!(e.code(), "invalid", "{e}");
    assert!(e.to_string().contains("file name"), "{e}");
    assert!(e.hint().unwrap().contains("--socket"));
}
