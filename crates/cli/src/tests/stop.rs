//! The daemon's own commands, `slinger daemon info` and `slinger daemon
//! stop`, against a real daemon.

use super::{cli_on, json, with_daemon};
use crate::output::exit;

#[test]
fn info_prints_one_field_per_line_and_json_has_them_all() {
    let (text, data) = with_daemon(|socket| {
        (cli_on(socket, &["daemon", "info"]), cli_on(socket, &["daemon", "info", "--json"]))
    });
    assert_eq!((text.code, text.err.as_str()), (0, ""));
    let names: Vec<&str> = text.out.lines().map(|l| l.split('\t').next().unwrap()).collect();
    let fields = [
        "version",
        "build",
        "protocol",
        "pid",
        "started_ms",
        "exe",
        "socket",
        "data_dir",
        "claude",
        "slinger",
        "log",
    ];
    assert_eq!(names, fields, "{}", text.out);
    let pid = format!("pid\t{}", std::process::id());
    assert!(text.out.lines().any(|l| l == pid), "{}", text.out);
    assert!(text.out.lines().any(|l| l == "log\t-"), "{}", text.out);
    assert!(text.out.lines().any(|l| l == "claude\t/nonexistent/claude"), "{}", text.out);

    let data = json(&data);
    assert_eq!(data["pid"], std::process::id());
    assert_eq!(data["version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(data["build"], slinger_control::stamp());
    assert_eq!(data["log"], serde_json::Value::Null);
    for field in fields {
        assert!(data.get(field).is_some(), "{field}: {data}");
    }
}

#[test]
fn stop_says_nothing_and_the_daemon_is_gone_after_it() {
    // with_daemon returns once the server has ended; nothing answers after.
    let (stopped, socket) =
        with_daemon(|socket| (cli_on(socket, &["daemon", "stop"]), socket.to_owned()));
    assert_eq!((stopped.code, stopped.out.as_str(), stopped.err.as_str()), (0, "", ""));
    assert!(!socket.exists());
    assert_eq!(cli_on(&socket, &["daemon", "info"]).code, exit::UNAVAILABLE);
}

#[test]
fn stop_json_names_the_pid_that_stops() {
    let run = with_daemon(|socket| cli_on(socket, &["daemon", "stop", "--json"]));
    assert_eq!(run.code, 0, "{}", run.err);
    assert_eq!(json(&run)["pid"], std::process::id());
}
