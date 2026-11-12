//! The calling agent's name from `SLINGER_AGENT` and the socket from
//! `SLINGER_SOCKET`, through the real `slinger` binary: each run gets an
//! environment of its own, which in-process tests cannot have.

use std::path::Path;
use std::process::{Command, Output};

use slinger_control::{Client, EventFilter};

const USAGE: i32 = 2;

/// Runs `slinger` on the socket with `SLINGER_AGENT` set to `agent`, or unset.
#[expect(clippy::unwrap_used, reason = "a test helper: a binary that cannot run fails the test")]
fn slinger(socket: &Path, agent: Option<&str>, args: &[&str]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_slinger"));
    command.args(args).arg("--socket").arg(socket).env_remove("SLINGER_AGENT");
    if let Some(agent) = agent {
        command.env("SLINGER_AGENT", agent);
    }
    command.output().unwrap()
}

#[test]
fn agent_verbs_take_the_agent_from_the_flag_or_the_environment_or_fail_as_usage() {
    let data = tempfile::tempdir().unwrap();
    // The socket in a directory of its own, removed on drop.
    let sockets = tempfile::tempdir().unwrap();
    let socket = sockets.path().join("d.sock");
    let store = slingerd::Store::open(data.path(), "my-laptop").unwrap();
    let server = slingerd::Server::bind(&socket).unwrap();
    let stopper = server.stopper();
    let serving = std::thread::spawn(move || server.serve(&store));
    let none = slinger(&socket, None, &["status", "set", "done"]);
    assert_eq!(none.status.code(), Some(USAGE));
    let err = String::from_utf8_lossy(&none.stderr);
    assert!(err.contains("--agent") && err.contains("SLINGER_AGENT"), "{err}");
    let from_env = slinger(&socket, Some("scout"), &["status", "set", "done"]);
    assert_eq!((from_env.status.code(), from_env.stdout.as_slice()), (Some(0), &b""[..]));
    let flag_wins = slinger(&socket, Some("scout"), &["status", "set", "done", "--agent", "ada"]);
    assert_eq!(flag_wins.status.code(), Some(0));

    let events = Client::new(&socket).record_list(EventFilter::default()).unwrap();
    let agents: Vec<&str> = events.iter().filter_map(|e| e["tags"]["agent"].as_str()).collect();
    assert_eq!(agents, ["scout", "ada"]);

    // `canvas put` names its agent the same way.
    let client = Client::new(&socket);
    client.canvas_create("main").unwrap();
    let bx = serde_json::json!({ "op": "add", "item": { "page": "p1", "kind": "box", "x": 0, "y": 0, "w": 10, "h": 10 } });
    let ops = slinger_control::CanvasOps { canvas: "main".into(), by: "user".into(), ops: bx };
    client.canvas_apply(ops).unwrap();
    let put = ["canvas", "put", "main", "--beside", "i1", "--text", "hi"];
    assert_eq!(slinger(&socket, None, &put).status.code(), Some(USAGE));
    assert_eq!(slinger(&socket, Some("scout"), &put).status.code(), Some(0));
    let shown = client.canvas_show("main", None).unwrap();
    assert_eq!(shown.items[1]["by"], "agent:scout");

    apply_undo_and_redo_take_the_agent_from_the_environment(&socket, &client);

    // An agent's process finds the daemon that started it by SLINGER_SOCKET.
    let mut list = Command::new(env!("CARGO_BIN_EXE_slinger"));
    let list = list.args(["canvas", "list"]).env("SLINGER_SOCKET", &socket).output().unwrap();
    assert_eq!((list.status.code(), list.stdout.as_slice()), (Some(0), &b"main\n"[..]));
    let mut elsewhere = Command::new(env!("CARGO_BIN_EXE_slinger"));
    elsewhere
        .args(["canvas", "list", "--socket", "/nonexistent/slinger.sock"])
        .env("SLINGER_SOCKET", &socket);
    let elsewhere = elsewhere.output().unwrap();
    assert_eq!(elsewhere.status.code(), Some(6), "--socket wins over SLINGER_SOCKET");
    stopper.stop();
    serving.join().unwrap().unwrap();
}

/// `canvas apply`, `undo` and `redo` act as `$SLINGER_AGENT` too, and as the
/// user without it. The canvas `main` holds a user's box and scout's text.
fn apply_undo_and_redo_take_the_agent_from_the_environment(socket: &Path, client: &Client) {
    let text = r#"{"op":"add","item":{"page":"p1","kind":"text","x":0,"y":900,"text":"a"}}"#;
    assert_eq!(
        slinger(socket, Some("scout"), &["canvas", "apply", "main", text]).status.code(),
        Some(0)
    );
    assert_eq!(slinger(socket, None, &["canvas", "apply", "main", text]).status.code(), Some(0));
    // The makers of the page's items, in drawing order; none when the show fails.
    let makers = |client: &Client| -> Vec<String> {
        let shown = client.canvas_show("main", None).map(|shown| shown.items);
        shown
            .unwrap_or_default()
            .iter()
            .map(|i| i["by"].as_str().unwrap_or_default().to_string())
            .collect()
    };
    assert_eq!(makers(client), ["user", "agent:scout", "agent:scout", "user"]);
    assert_eq!(slinger(socket, Some("scout"), &["canvas", "undo", "main"]).status.code(), Some(0));
    assert_eq!(
        makers(client),
        ["user", "agent:scout", "user"],
        "the agent's undo takes its own step"
    );
    assert_eq!(slinger(socket, Some("scout"), &["canvas", "redo", "main"]).status.code(), Some(0));
    assert_eq!(makers(client).iter().filter(|by| *by == "agent:scout").count(), 2);
}
