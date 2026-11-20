//! Closing a workspace through the real socket, with the stand-in for Claude
//! Code: its agent's process ends with it, unless another open workspace
//! keeps that agent (slinger-0f6923).

use std::process::Command;

use tempfile::TempDir;

use super::Daemon;
use super::process::fake_claude;
use super::session::open_with_agent;

/// Whether a process with this id still exists.
fn alive(pid: u32) -> bool {
    let mut kill = Command::new("kill");
    kill.arg("-0").arg(pid.to_string()).stderr(std::process::Stdio::null());
    kill.status().unwrap().success()
}

#[test]
fn closing_a_workspace_ends_its_agents_process_and_its_working() {
    let (data, projects) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    let daemon = Daemon::start_with_claude(data.path(), &fake_claude());
    let client = daemon.client();
    let home = open_with_agent(&client, &projects, "notes", "scout");
    let pid = client.session_start("notes").unwrap().pid.unwrap();
    client.status_set("scout", "done", Some("wrote the plan")).unwrap();
    client.status_set("scout", "working", None).unwrap();

    assert_eq!(client.workspace_close("notes").unwrap(), home);
    assert!(!alive(pid), "the agent's process ended with its workspace");
    assert_eq!(client.session_list().unwrap(), []);

    client.workspace_open(Some(&home), None).unwrap();
    let shown = client.session_show("scout").unwrap();
    let seen = (shown.process, shown.pid, shown.status.as_deref());
    assert_eq!(seen, (None, None, Some("done")), "no process, the status from before");
    let again = client.session_start("notes").unwrap();
    assert_ne!(again.pid, Some(pid), "a new start runs a new process");
    daemon.stop();
}

#[test]
fn an_agent_another_open_workspace_keeps_runs_on() {
    let (data, projects) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    let daemon = Daemon::start_with_claude(data.path(), &fake_claude());
    let client = daemon.client();
    open_with_agent(&client, &projects, "notes", "scout");
    open_with_agent(&client, &projects, "drafts", "scout");
    let pid = client.session_start("notes").unwrap().pid.unwrap();

    client.workspace_close("drafts").unwrap();
    assert!(alive(pid), "notes still has scout");
    assert_eq!(client.session_show("scout").unwrap().pid, Some(pid));

    client.workspace_close("notes").unwrap();
    assert!(!alive(pid), "the last workspace with scout closed");
    daemon.stop();
}
