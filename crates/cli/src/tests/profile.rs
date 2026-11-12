//! `slinger profile ...`, `session start --profile` and `sling send
//! --profile` against a real daemon.

use super::{cli_on, json, with_daemon_running};
use crate::output::exit;

#[test]
fn profiles_are_set_listed_and_removed_and_a_session_starts_under_one() {
    let (projects, bin) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let dir = projects.path().join("notes");
    std::fs::create_dir(&dir).unwrap();
    let dir = dir.canonicalize().unwrap();
    let src = projects.path().canonicalize().unwrap().join("src");
    std::fs::create_dir(&src).unwrap();
    let file = "version = 1\nname = \"notes\"\n[agent]\nname = \"scout\"\n";
    std::fs::write(dir.join("slinger-workspace.toml"), file).unwrap();
    // A stand-in for Claude Code: it keeps every line it is sent, where it runs.
    let claude = bin.path().join("claude");
    std::fs::write(&claude, "#!/bin/sh\nexec cat >> in.txt\n").unwrap();
    std::fs::set_permissions(&claude, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    let (path, src_path) = (dir.to_str().unwrap(), src.to_str().unwrap());
    let runs = with_daemon_running(&claude, |socket| {
        let run = |args: &[&str]| cli_on(socket, args);
        vec![
            run(&["workspace", "open", path]),
            run(&["profile", "list", "notes"]),
            run(&[
                "profile",
                "set",
                "notes",
                "dev",
                "--dir",
                src_path,
                "--tool",
                "Edit",
                "--tool",
                "Bash(git *)",
            ]),
            run(&["profile", "set", "notes", "look"]),
            run(&["profile", "list", "notes"]),
            run(&["profile", "remove", "notes", "look", "--json"]),
            run(&["profile", "set", "notes", "dev", "--tool", "Sudo"]),
            run(&["profile", "remove", "notes", "look"]),
            run(&["session", "start", "notes", "--profile", "nope"]),
            run(&["session", "start", "notes", "--profile", "dev"]),
            run(&["session", "send", "notes", "hi"]),
            run(&["session", "show", "scout"]),
            run(&["sling", "send", "main", "--profile", "look"]),
        ]
    });
    let codes: Vec<u8> = runs.iter().map(|r| r.code).collect();
    assert_eq!(
        codes,
        [
            0,
            0,
            0,
            0,
            0,
            0,
            exit::INVALID,
            exit::NOT_FOUND,
            exit::NOT_FOUND,
            0,
            0,
            0,
            exit::NOT_FOUND
        ]
    );
    for r in [&runs[1], &runs[2], &runs[3], &runs[9], &runs[10]] {
        assert_eq!((r.out.as_str(), r.err.as_str()), ("", ""));
    }
    assert_eq!(runs[4].out, format!("dev\t{src_path}\tEdit, Bash(git *)\nlook\t-\t-\n"));
    let removed = json(&runs[5]);
    assert_eq!(removed["command"], "profile remove");
    assert_eq!(removed["profiles"][0]["tools"][1], "Bash(git *)");
    assert!(runs[6].err.contains("Bash, Edit"), "{}", runs[6].err);
    assert!(runs[11].out.contains("\nprofile\tdev\n"), "{}", runs[11].out);
    let sent = std::fs::read_to_string(src.join("in.txt")).unwrap();
    assert!(sent.contains("\"hi\""), "the session runs in the profile's directory: {sent}");
}
