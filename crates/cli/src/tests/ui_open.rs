//! `slinger ui open` against a real daemon: the showing's id and pane, the
//! data from the flag or stdin, and the exit codes of what it refuses.

use super::{cli_as, cli_on, cli_raw, json, with_daemon};
use crate::output::exit;

#[test]
fn open_prints_the_showing_and_its_pane_and_takes_data_from_stdin() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("pick.html");
    std::fs::write(&file, "<p>pick</p>").unwrap();
    let runs = with_daemon(|socket| {
        let mut runs = vec![
            cli_on(socket, &["workspace", "open", "--name", "notes"]),
            cli_on(socket, &["ui", "put", "pick", file.to_str().unwrap()]),
        ];
        let open = ["ui", "open", "pick", "--workspace", "notes", "--agent", "scout"];
        let data = [&open[..], &["--data", r#"{"slots":["Tue"]}"#, "--beside", "1"]].concat();
        runs.push(cli_on(socket, &data));
        let stdin = [&open[..], &["--data", "-", "--below", "--json"]].concat();
        runs.push(cli_raw(socket, &stdin, b"[1, 2]", &mut Vec::new()));
        runs.push(cli_on(socket, &["record", "list", "--kind", "ui_opened"]));
        for bad in [
            &[&open[..], &["--data", "{"]].concat()[..],
            &["ui", "open", "nope", "--workspace", "notes", "--agent", "scout"],
            &["ui", "open", "pick", "--workspace", "notes", "--agent", "scout", "--as", "canvas"],
        ] {
            runs.push(cli_on(socket, bad));
        }
        runs
    });
    for r in &runs[..5] {
        assert_eq!((r.code, r.err.as_str()), (exit::OK, ""), "{}", r.out);
    }
    let (id, pane) = runs[2].out.trim_end().split_once('\t').unwrap();
    assert!(id.starts_with("pick#"), "{id}");
    assert_eq!(pane, "2");
    let second = json(&runs[3]);
    assert_eq!((&second["command"], &second["pane"]), (&"ui open".into(), &3.into()));
    assert_eq!(second["ui"]["name"], "pick");
    let events: Vec<&str> = runs[4].out.lines().collect();
    assert_eq!(events.len(), 2);
    assert!(events[0].contains(r#""data":{"slots":["Tue"]}"#), "{}", events[0]);
    assert!(events[1].contains(r#""data":[1,2]"#), "{}", events[1]);
    assert_eq!(runs[5].code, exit::INVALID);
    assert!(runs[5].err.contains("not JSON"), "{}", runs[5].err);
    assert_eq!(runs[6].code, exit::NOT_FOUND);
    assert_eq!(runs[7].code, exit::USAGE, "only a pane, for now");
}

#[test]
fn open_as_an_item_prints_the_showing_and_the_item_beside_the_one_named() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("pick.html");
    std::fs::write(&file, "<p>pick</p>").unwrap();
    let bx = r#"{"op":"add","item":{"page":"p1","kind":"box","x":0,"y":0,"w":1000,"h":500}}"#;
    let runs = with_daemon(|socket| {
        let open = ["ui", "open", "pick", "--as", "item", "--agent", "scout"];
        let mut runs = vec![
            cli_on(socket, &["canvas", "create", "notes"]),
            cli_on(socket, &["canvas", "apply", "notes", bx]),
            cli_on(socket, &["ui", "put", "pick", file.to_str().unwrap()]),
            cli_on(socket, &[&open[..], &["--canvas", "notes", "--beside", "i1"]].concat()),
        ];
        let sized = ["--canvas", "notes", "--beside", "b1", "--size", "100,50", "--json"];
        runs.push(cli_on(socket, &[&open[..], &sized].concat()));
        runs.push(cli_on(socket, &[&open[..], &["--beside", "i1"]].concat()));
        runs.push(cli_on(socket, &[&open[..], &["--canvas", "notes", "--beside", "i9"]].concat()));
        let pane = ["ui", "open", "pick", "--workspace", "notes", "--beside", "i1"];
        runs.push(cli_on(socket, &[&pane[..], &["--agent", "scout"]].concat()));
        runs
    });
    for r in &runs[..5] {
        assert_eq!((r.code, r.err.as_str()), (exit::OK, ""), "{}", r.out);
    }
    let (id, item) = runs[3].out.trim_end().split_once('\t').unwrap();
    assert!(id.starts_with("pick#"), "{id}");
    assert_eq!(item, "i2");
    let sized = json(&runs[4]);
    assert_eq!((&sized["item"]["item"], &sized["item"]["w"]), (&"i3".into(), &100.into()));
    assert!(sized.get("pane").is_none(), "{sized}");
    assert_eq!(runs[5].code, exit::USAGE, "an item needs its canvas");
    assert_eq!(runs[6].code, exit::NOT_FOUND);
    assert_eq!(runs[7].code, exit::INVALID, "a pane is beside a pane's number");
}

/// A session slingerd started names only its agent: a pane opens in that
/// agent's workspace, and an agent with none is told to pass --workspace
/// (slinger-38553d).
#[test]
fn open_without_a_workspace_opens_in_the_calling_agents() {
    let projects = tempfile::tempdir().unwrap();
    let dir = projects.path().join("notes");
    std::fs::create_dir(&dir).unwrap();
    let file = "version = 1\nname = \"notes\"\n[agent]\nname = \"scout\"\n";
    std::fs::write(dir.join("slinger-workspace.toml"), file).unwrap();
    std::fs::write(dir.join("pick.html"), "<p>pick</p>").unwrap();
    let runs = with_daemon(|socket| {
        let path = dir.to_str().unwrap();
        vec![
            cli_on(socket, &["workspace", "open", path]),
            cli_on(socket, &["ui", "put", "pick", dir.join("pick.html").to_str().unwrap()]),
            cli_as("scout", socket, &["ui", "open", "pick"]),
            cli_on(socket, &["pane", "list", "notes"]),
            cli_as("nobody", socket, &["ui", "open", "pick"]),
        ]
    });
    for r in &runs[..4] {
        assert_eq!((r.code, r.err.as_str()), (exit::OK, ""), "{}", r.out);
    }
    let (id, pane) = runs[2].out.trim_end().split_once('\t').unwrap();
    assert!(id.starts_with("pick#"), "{id}");
    assert!(runs[3].out.lines().any(|l| l.starts_with(&format!("{pane}\t"))), "{}", runs[3].out);
    assert_eq!(runs[4].code, exit::NOT_FOUND);
    assert!(
        runs[4].err.contains("nobody") && runs[4].err.contains("--workspace"),
        "{}",
        runs[4].err
    );
}
