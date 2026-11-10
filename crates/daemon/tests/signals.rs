//! The real binary: it serves, and SIGTERM ends it cleanly.

use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use slinger_control::Client;

#[test]
fn sigterm_stops_the_daemon_cleanly() {
    let socket: PathBuf = std::env::temp_dir().join(format!("s-{}.sock", std::process::id()));
    let daemon = Command::new(env!("CARGO_BIN_EXE_slingerd"))
        .arg("--socket")
        .arg(&socket)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    let client = Client::new(&socket);
    let deadline = Instant::now() + Duration::from_secs(10);
    let greeting = loop {
        match client.hello("ada") {
            Ok(greeting) => break greeting,
            Err(e) if Instant::now() > deadline => panic!("the daemon never answered: {e}"),
            Err(_) => std::thread::sleep(Duration::from_millis(20)),
        }
    };
    assert_eq!(greeting, "hello, ada");

    let killed = Command::new("kill").arg("-TERM").arg(daemon.id().to_string()).status().unwrap();
    assert!(killed.success());
    let output = daemon.wait_with_output().unwrap();
    assert!(output.status.success(), "{:?}", output.status);
    // Silence is golden: a clean run says nothing.
    assert_eq!((output.stdout.as_slice(), output.stderr.as_slice()), (&b""[..], &b""[..]));
    assert!(!socket.exists());
    std::fs::remove_file(format!("{}.lock", socket.display())).unwrap();
}
