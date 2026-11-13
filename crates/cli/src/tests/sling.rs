//! `slinger sling send` against a real daemon, with a stand-in for Claude Code.

use serde_json::Value;

use super::{cli_on, cli_raw, json, with_daemon_running};
use crate::output::exit;

#[test]
fn a_sling_waits_for_a_listener_or_goes_to_the_running_agent_quietly() {
    let projects = tempfile::tempdir().unwrap();
    let dir = projects.path().join("notes");
    std::fs::create_dir(&dir).unwrap();
    let dir = dir.canonicalize().unwrap();
    let file = "version = 1\nname = \"notes\"\ncanvas = \"notes\"\n[agent]\nname = \"scout\"\n";
    std::fs::write(dir.join("slinger-workspace.toml"), file).unwrap();
    let stroke =
        r#"{"op":"add","item":{"page":"p1","kind":"stroke","width":20,"pts":[0,0,100,50]}}"#;
    let path = dir.to_str().unwrap();
    let runs = with_daemon_running(|socket| {
        let mut runs = vec![
            cli_on(socket, &["workspace", "open", path]),
            cli_on(socket, &["canvas", "apply", "notes", stroke]),
            cli_on(socket, &["sling", "send", "notes", "--prompt", "what?", "--json"]),
            cli_on(socket, &["session", "show", "scout"]),
            cli_on(socket, &["session", "listen", "notes", "--wait", "0"]),
            cli_on(socket, &["session", "listen", "notes", "--wait", "0"]),
            cli_on(socket, &["session", "listen", "notes", "--wait", "0", "--json"]),
            cli_on(socket, &["session", "listen", "nowhere"]),
            cli_on(socket, &["canvas", "apply", "notes", stroke]),
            cli_on(socket, &["session", "start", "notes"]),
        ];
        let args = ["sling", "send", "notes", "--select", "i2", "--prompt", "-"];
        runs.push(cli_raw(socket, &args, b"what is this?", &mut Vec::new()));
        runs.push(cli_on(socket, &["sling", "send", "notes", "--json"]));
        runs.push(cli_on(socket, &["sling", "send", "nowhere"]));
        runs
    });
    let codes: Vec<u8> = runs.iter().map(|r| r.code).collect();
    assert_eq!(codes, [0, 0, 0, 0, 0, 0, 0, exit::NOT_FOUND, 0, 0, 0, 0, exit::NOT_FOUND]);
    // With no running agent the sling waits for a listener, who takes it as text.
    assert_eq!(json(&runs[2])["deliver"], "listen");
    assert!(runs[3].out.contains("deliver\tlisten\nstatus\t-\nlistening\tfalse\npending\t1\n"));
    let heard = &runs[4].out;
    assert!(heard.starts_with("slinger-sling 1 canvas=notes agent=scout\n"), "{heard}");
    assert!(heard.ends_with("# prompt\nwhat?\n"), "{heard}");
    // The daemon's data is gone by now: its own tests check the image files.
    assert!(heard.lines().nth(1).unwrap().contains(" at=/"), "{heard}");
    assert_eq!((runs[5].out.as_str(), runs[5].err.as_str()), ("", ""), "none waits");
    let none = json(&runs[6]);
    assert_eq!((&none["command"], &none["agent"]), (&"session listen".into(), &"scout".into()));
    assert!(none.get("sling").is_none(), "{none}");
    // A started process takes the next ones; i1 is the canvas's button.
    assert_eq!((runs[10].out.as_str(), runs[10].err.as_str()), ("", ""));
    let slung = json(&runs[11]);
    assert_eq!((&slung["command"], &slung["strokes"]), (&"sling send".into(), &0.into()));
    assert_eq!((&slung["agent"], &slung["deliver"]), (&"scout".into(), &"process".into()));
    let sent = std::fs::read_to_string(dir.join("in.txt")).unwrap();
    let first: Value = serde_json::from_str(sent.lines().next().unwrap()).unwrap();
    let blocks = first["message"]["content"].as_array().unwrap();
    let text = blocks.iter().filter_map(|b| b["text"].as_str()).collect::<String>();
    assert!(text.starts_with("slinger-sling 1 canvas=notes selected=c2\n"), "{text}");
    assert!(text.ends_with("# prompt\nwhat is this?"), "{text}");
}
