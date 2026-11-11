//! `slinger session ...` and `slinger status set` against a real daemon. The
//! name from `SLINGER_AGENT` is tested against the real binary, in
//! `tests/agent_env.rs`: these tests run in one process and share its
//! environment.

use serde_json::Value;

use super::{cli_each, cli_on, cli_raw, json, with_daemon_running};
use crate::output::exit;

#[test]
fn a_status_set_shows_in_the_session_list_and_the_record() {
    let projects = tempfile::tempdir().unwrap();
    let dir = projects.path().join("notes");
    std::fs::create_dir(&dir).unwrap();
    let dir = dir.canonicalize().unwrap();
    let file = "version = 1\nname = \"notes\"\n[agent]\nname = \"scout\"\n";
    std::fs::write(dir.join("slinger-workspace.toml"), file).unwrap();
    let path = dir.to_str().unwrap();
    let runs = cli_each(&[
        &["workspace", "open", path],
        &["session", "list"],
        &["status", "set", "blocked", "--note", "need the key", "--agent", "scout"],
        &["session", "list"],
        &["session", "show", "scout"],
        &["record", "list", "--agent", "scout"],
        &["status", "set", "working", "--agent", "scout", "--json"],
        &["session", "show", "scout", "--json"],
    ]);
    for r in &runs {
        assert_eq!((r.code, r.err.as_str()), (exit::OK, ""), "{}", r.out);
    }
    let out: Vec<&str> = runs.iter().map(|r| r.out.as_str()).collect();
    assert_eq!(out[1], format!("scout\t-\t{path}\n"));
    assert_eq!(out[2], "");
    assert_eq!(out[3], format!("scout\tblocked\t{path}\tneed the key\n"));
    assert_eq!(
        out[4],
        format!(
            "agent\tscout\nhome\t{path}\ndeliver\tprocess\nstatus\tblocked\nnote\tneed the key\n"
        )
    );
    assert!(
        out[5].contains("\tagent_blocked\tmachine=my-laptop workspace=notes agent=scout\t"),
        "{}",
        out[5]
    );
    let set = json(&runs[6]);
    assert_eq!(
        (&set["command"], &set["status"], &set["agent"]),
        (&"status set".into(), &"working".into(), &"scout".into())
    );
    assert!(set.get("event").is_none(), "{set}");
    let shown = json(&runs[7]);
    assert_eq!(shown["session"]["status"], "working");
    assert_eq!(shown["session"]["home"], path);
}

#[test]
fn bad_statuses_and_unknown_sessions_get_their_exit_codes() {
    let runs = cli_each(&[
        &["status", "set", "idle", "--agent", "scout"],
        &["session", "show", "nobody"],
        &["status", "set", "done", "--agent", " "],
    ]);
    let codes: Vec<u8> = runs.iter().map(|r| r.code).collect();
    assert_eq!(codes, [exit::USAGE, exit::NOT_FOUND, exit::USAGE]);
    assert!(runs[2].err.contains("SLINGER_AGENT"), "{}", runs[2].err);
}

#[test]
fn a_session_starts_takes_messages_and_stops_quietly() {
    let (projects, bin) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let dir = projects.path().join("notes");
    std::fs::create_dir(&dir).unwrap();
    let dir = dir.canonicalize().unwrap();
    let file = "version = 1\nname = \"notes\"\n[agent]\nname = \"scout\"\n";
    std::fs::write(dir.join("slinger-workspace.toml"), file).unwrap();
    // A stand-in for Claude Code: it keeps every line it is sent, in its home.
    let claude = bin.path().join("claude");
    std::fs::write(&claude, "#!/bin/sh\nexec cat >> in.txt\n").unwrap();
    std::fs::set_permissions(&claude, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    let path = dir.to_str().unwrap();
    let runs = with_daemon_running(&claude, |socket| {
        let mut runs = vec![
            cli_on(socket, &["session", "send", "notes", "early"]),
            cli_on(socket, &["workspace", "open", path]),
            cli_on(socket, &["session", "send", "notes", "early"]),
            cli_on(socket, &["session", "start", "notes"]),
            cli_on(socket, &["session", "send", "notes", "one"]),
        ];
        runs.push(cli_raw(socket, &["session", "send", "notes", "-"], b"two\n", &mut Vec::new()));
        runs.push(cli_on(socket, &["session", "show", "scout", "--json"]));
        runs.push(cli_on(socket, &["session", "stop", "notes"]));
        runs
    });
    let codes: Vec<u8> = runs.iter().map(|r| r.code).collect();
    assert_eq!(codes, [exit::NOT_FOUND, exit::OK, exit::UNAVAILABLE, 0, 0, 0, 0, 0]);
    assert!(runs[2].err.contains("session start"), "{}", runs[2].err);
    for r in &runs[3..6] {
        assert_eq!((r.out.as_str(), r.err.as_str()), ("", ""));
    }
    let shown = json(&runs[6]);
    assert_eq!(shown["session"]["process"], "running");
    assert_eq!(runs[7].out, "");
    let sent = std::fs::read_to_string(dir.join("in.txt")).unwrap();
    let texts: Vec<Value> = sent.lines().map(|l| serde_json::from_str(l).unwrap()).collect();
    let texts: Vec<&str> =
        texts.iter().map(|t| t["message"]["content"].as_str().unwrap()).collect();
    assert_eq!(texts, ["one", "two\n"]);
}
