//! `slinger session ...` and `slinger status set` against a real daemon. The
//! name from `SLINGER_AGENT` is tested against the real binary, in
//! `tests/agent_env.rs`: these tests run in one process and share its
//! environment.

use super::{cli_each, json};
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
