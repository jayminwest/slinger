//! `slinger sling send` against a real daemon, with a stand-in for Claude Code.

use serde_json::Value;

use super::{cli_on, cli_raw, json, with_daemon_running};
use crate::output::exit;

#[test]
fn a_sling_goes_to_the_running_agent_quietly_and_without_one_exits_6() {
    let (projects, bin) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let dir = projects.path().join("notes");
    std::fs::create_dir(&dir).unwrap();
    let dir = dir.canonicalize().unwrap();
    let file = "version = 1\nname = \"notes\"\ncanvas = \"notes\"\n[agent]\nname = \"scout\"\n";
    std::fs::write(dir.join("slinger-workspace.toml"), file).unwrap();
    // A stand-in for Claude Code: it keeps every line it is sent, in its home.
    let claude = bin.path().join("claude");
    std::fs::write(&claude, "#!/bin/sh\nexec cat >> in.txt\n").unwrap();
    std::fs::set_permissions(&claude, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    let stroke =
        r#"{"op":"add","item":{"page":"p1","kind":"stroke","width":20,"pts":[0,0,100,50]}}"#;
    let path = dir.to_str().unwrap();
    let runs = with_daemon_running(&claude, |socket| {
        let mut runs = vec![
            cli_on(socket, &["workspace", "open", path]),
            cli_on(socket, &["canvas", "apply", "notes", stroke]),
            cli_on(socket, &["sling", "send", "notes", "--prompt", "what?"]),
            cli_on(socket, &["session", "start", "notes"]),
        ];
        let args = ["sling", "send", "notes", "--select", "i1", "--prompt", "-"];
        runs.push(cli_raw(socket, &args, b"what is this?", &mut Vec::new()));
        runs.push(cli_on(socket, &["sling", "send", "notes", "--json"]));
        runs.push(cli_on(socket, &["sling", "send", "nowhere"]));
        runs
    });
    let codes: Vec<u8> = runs.iter().map(|r| r.code).collect();
    assert_eq!(codes, [0, 0, exit::UNAVAILABLE, 0, 0, 0, exit::NOT_FOUND]);
    assert!(runs[2].err.contains("session start"), "{}", runs[2].err);
    assert_eq!((runs[4].out.as_str(), runs[4].err.as_str()), ("", ""));
    let slung = json(&runs[5]);
    assert_eq!((&slung["command"], &slung["strokes"]), (&"sling send".into(), &0.into()));
    assert_eq!(slung["agent"], "scout");
    let sent = std::fs::read_to_string(dir.join("in.txt")).unwrap();
    let first: Value = serde_json::from_str(sent.lines().next().unwrap()).unwrap();
    let blocks = first["message"]["content"].as_array().unwrap();
    let text = blocks.iter().filter_map(|b| b["text"].as_str()).collect::<String>();
    assert!(text.starts_with("slinger-sling 1 canvas=notes selected=c1\n"), "{text}");
    assert!(text.ends_with("# prompt\nwhat is this?"), "{text}");
}
