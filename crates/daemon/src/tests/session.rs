//! Sessions through the real socket: the agents of the open workspaces, the
//! status each sets, its events in the record, and a restart.

use std::path::PathBuf;

use serde_json::json;
use slinger_control::{Client, EventFilter, Session};
use tempfile::TempDir;

use super::Daemon;

/// Opens a workspace in a new directory of `parent` whose agent is `agent`,
/// and returns the directory.
pub(super) fn open_with_agent(
    client: &Client,
    parent: &TempDir,
    name: &str,
    agent: &str,
) -> PathBuf {
    let dir = parent.path().join(name);
    std::fs::create_dir_all(&dir).unwrap();
    let dir = dir.canonicalize().unwrap();
    let file = format!(
        "version = 1\nname = \"{name}\"\ncanvas = \"{name}\"\n[agent]\nname = \"{agent}\"\n"
    );
    std::fs::write(dir.join("slinger-workspace.toml"), file).unwrap();
    client.workspace_open(Some(&dir), None).unwrap();
    dir
}

fn status(session: &Session) -> (Option<&str>, Option<&str>) {
    (session.status.as_deref(), session.note.as_deref())
}

fn agent_events(client: &Client, agent: &str) -> Vec<serde_json::Value> {
    let filter = EventFilter { agent: Some(agent.into()), ..EventFilter::default() };
    client.record_list(filter).unwrap()
}

#[test]
fn status_changes_show_in_the_session_list_and_the_record() {
    let (data, projects) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    let daemon = Daemon::start_in(data.path());
    let client = daemon.client();
    assert_eq!(client.session_list().unwrap(), []);
    let home = open_with_agent(&client, &projects, "notes", "scout");
    open_with_agent(&client, &projects, "plain", "ada");

    let sessions = client.session_list().unwrap();
    let agents: Vec<&str> = sessions.iter().map(|s| s.agent.as_str()).collect();
    assert_eq!(agents, ["ada", "scout"]);
    let scout = &sessions[1];
    assert_eq!(
        (&scout.home, scout.deliver.as_str(), status(scout)),
        (&home, "process", (None, None))
    );
    blocked_shows_and_is_recorded(&client);
    working_shows_but_is_not_recorded(&client);
    client.status_set("scout", "done", Some("wrote the plan")).unwrap();
    let events = agent_events(&client, "scout");
    assert_eq!((events.len(), &events[1]["kind"]), (2, &json!("agent_done")));
    assert_eq!(events[1]["payload"], json!({ "summary": "wrote the plan" }));
    assert_eq!(status(&client.session_list().unwrap()[0]), (None, None), "ada set nothing");
    daemon.stop();

    let daemon = Daemon::start_in(data.path());
    let shown = daemon.client().session_show("scout").unwrap();
    assert_eq!(status(&shown), (Some("done"), Some("wrote the plan")), "the record keeps it");
    daemon.stop();
}

fn blocked_shows_and_is_recorded(client: &Client) {
    let set = client.status_set("scout", "blocked", Some("need the key")).unwrap();
    assert_eq!((set.status.as_str(), set.note.as_deref()), ("blocked", Some("need the key")));
    let shown = client.session_show("scout").unwrap();
    assert_eq!(status(&shown), (Some("blocked"), Some("need the key")));
    let events = agent_events(client, "scout");
    assert_eq!(events.len(), 1);
    assert_eq!(Some(events[0]["number"].as_u64().unwrap()), set.event);
    assert_eq!(events[0]["kind"], "agent_blocked");
    assert_eq!(events[0]["source"], "scout");
    assert_eq!(events[0]["tags"]["workspace"], "notes");
    assert_eq!(events[0]["payload"], json!({ "reason": "need the key" }));
}

fn working_shows_but_is_not_recorded(client: &Client) {
    let set = client.status_set("scout", "working", None).unwrap();
    assert_eq!((set.status.as_str(), set.event), ("working", None));
    assert_eq!(status(&client.session_show("scout").unwrap()), (Some("working"), None));
    assert_eq!(agent_events(client, "scout").len(), 1, "working is not in the record");
}

#[test]
fn an_agent_outside_the_workspaces_is_recorded_but_has_no_session() {
    let projects = TempDir::new().unwrap();
    let daemon = Daemon::start();
    let client = daemon.client();
    let set = client.status_set("drifter", "done", None).unwrap();
    assert!(set.event.is_some());
    let events = agent_events(&client, "drifter");
    assert_eq!((events.len(), events[0].get("payload")), (1, Some(&json!({}))));
    let err = client.session_show("drifter").unwrap_err();
    assert_eq!(err.code(), "not_found");
    assert!(err.hint().unwrap().contains("session list"));
    open_with_agent(&client, &projects, "late", "drifter");
    assert_eq!(status(&client.session_show("drifter").unwrap()), (Some("done"), None));
    daemon.stop();
}

#[test]
fn a_blank_agent_or_an_unknown_status_is_invalid_and_writes_nothing() {
    let daemon = Daemon::start();
    let client = daemon.client();
    for (agent, status) in [(" ", "done"), ("scout", "idle")] {
        let err = client.status_set(agent, status, None).unwrap_err();
        assert_eq!(err.code(), "invalid", "{agent:?} {status:?}");
        assert!(err.hint().is_some());
    }
    assert_eq!(
        client.record_list(EventFilter::default()).unwrap(),
        Vec::<serde_json::Value>::new()
    );
    daemon.stop();
}
