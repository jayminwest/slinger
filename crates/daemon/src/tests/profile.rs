//! Profiles through the real socket, with the stand-in for Claude Code: set,
//! listed and removed in the workspace's file; a session under one runs in
//! its directory with its rules and keeps a conversation of its own; and a
//! sling that names one reaches only a process under it.

use std::path::{Path, PathBuf};

use slinger_control::{Client, EventFilter, Profile, SlingAsk};
use tempfile::TempDir;

use super::Daemon;
use super::process::{fake_claude, wait_for, wait_lines};
use super::session::open_with_agent;

fn profile(name: &str, dir: Option<&Path>, tools: &[&str]) -> Profile {
    Profile {
        name: name.into(),
        dir: dir.map(|d| d.display().to_string()),
        tools: tools.iter().map(|t| (*t).to_string()).collect(),
    }
}

/// A new directory `name` under `parent`, links resolved.
fn dir(parent: &TempDir, name: &str) -> PathBuf {
    let dir = parent.path().join(name);
    std::fs::create_dir_all(&dir).unwrap();
    dir.canonicalize().unwrap()
}

fn names(client: &Client, workspace: &str) -> Vec<String> {
    let listed = client.profile_list(workspace).unwrap();
    listed.profiles.into_iter().map(|p| p.name).collect()
}

#[test]
fn profiles_are_set_listed_and_removed_in_the_workspace_file() {
    let (data, projects) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    let daemon = Daemon::start_in(data.path());
    let client = daemon.client();
    let home = open_with_agent(&client, &projects, "notes", "scout");
    let src = dir(&projects, "src");
    assert_eq!(names(&client, "notes"), Vec::<String>::new());

    let dev = profile("dev", Some(&src), &["Edit", "Bash(git *)"]);
    let set = client.profile_set("notes", dev.clone()).unwrap();
    assert_eq!((set.workspace.as_str(), set.profiles), ("notes", vec![dev]));
    client.profile_set("notes", profile("look", None, &[])).unwrap();
    let narrower = profile("dev", Some(&src), &["Write"]);
    let set = client.profile_set(home.to_str().unwrap(), narrower.clone()).unwrap();
    assert_eq!(set.profiles, [narrower.clone(), profile("look", None, &[])], "replaced in place");
    let file = std::fs::read_to_string(home.join("slinger-workspace.toml")).unwrap();
    assert_eq!(file.matches("[[agent.profile]]").count(), 2, "{file}");

    assert_eq!(client.profile_remove("notes", "look").unwrap().profiles, [narrower]);
    let err = client.profile_remove("notes", "look").unwrap_err();
    assert_eq!(err.code(), "not_found");
    assert!(err.hint().unwrap().contains("dev"), "{err:?}");

    // What it refuses, it refuses loudly and writes nothing.
    let before = std::fs::read_to_string(home.join("slinger-workspace.toml")).unwrap();
    let missing = projects.path().join("missing");
    for (bad, code) in [
        (profile("dev", None, &["Sudo"]), "invalid"),
        (profile("dev", None, &[""]), "invalid"),
        (profile("dev", None, &["Bash()"]), "invalid"),
        (profile("Dev", None, &[]), "invalid"),
        (profile("dev", Some(Path::new("src")), &[]), "invalid"),
        (profile("dev", Some(&missing), &[]), "not_found"),
    ] {
        let err = client.profile_set("notes", bad.clone()).unwrap_err();
        assert_eq!(err.code(), code, "{bad:?}: {err}");
        assert!(err.hint().is_some(), "{bad:?}");
    }
    let err = client.profile_set("notes", profile("dev", None, &["Sudo"])).unwrap_err();
    assert!(err.hint().unwrap().contains("Bash, Edit"), "{err:?}");
    assert_eq!(std::fs::read_to_string(home.join("slinger-workspace.toml")).unwrap(), before);

    assert_eq!(client.profile_list("nowhere").unwrap_err().code(), "not_found");
    let bare = client.workspace_open(Some(&dir(&projects, "bare")), None).unwrap();
    let err = client.profile_set(&bare.workspace.name, profile("dev", None, &[])).unwrap_err();
    assert_eq!(err.code(), "invalid", "no agent: {err}");
    daemon.stop();
}

#[test]
fn the_workspace_list_names_each_agent_s_profiles() {
    let (data, projects) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    let daemon = Daemon::start_in(data.path());
    let client = daemon.client();
    open_with_agent(&client, &projects, "notes", "scout");
    let agent = |client: &Client| client.workspace_list().unwrap()[0].agent.clone().unwrap();
    assert_eq!(agent(&client).profiles, Vec::<String>::new());
    client.profile_set("notes", profile("look", None, &[])).unwrap();
    client.profile_set("notes", profile("dev", None, &["Edit"])).unwrap();
    assert_eq!(agent(&client).profiles, ["look", "dev"], "in the file's order");
    daemon.stop();
}

/// Opens a workspace `notes` whose agent `scout` may also fetch the web, at
/// home in its primary root, with a profile `dev` in `src`.
fn open_with_profile(client: &Client, projects: &TempDir) -> (PathBuf, PathBuf) {
    let home = dir(projects, "notes");
    let file = format!(
        "version = 1\nname = \"notes\"\ncanvas = \"notes\"\n\
         [agent]\nname = \"scout\"\ntools = [\"WebFetch\"]\n\
         [[root]]\nmachine = \"test-machine\"\npath = \"{}\"\nprimary = true\n",
        home.display()
    );
    std::fs::write(home.join("slinger-workspace.toml"), file).unwrap();
    client.workspace_open(Some(&home), None).unwrap();
    let src = dir(projects, "src");
    client.profile_set("notes", profile("dev", Some(&src), &["Edit", "Bash(git *)"])).unwrap();
    (home, src)
}

#[test]
fn a_session_under_a_profile_runs_in_its_dir_with_its_rules_and_its_own_conversation() {
    let (data, projects) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    let daemon = Daemon::start_with_claude(data.path(), &fake_claude());
    let client = daemon.client();
    let (home, src) = open_with_profile(&client, &projects);

    let plain = client.session_start("notes").unwrap();
    let plain_id = plain.session_id.clone().unwrap();
    assert_eq!(plain.profile, None);
    assert!(
        wait_lines(&home.join("args.txt"), 1)[0]
            .contains("--allowedTools Bash(slinger *) Edit Write WebFetch --verbose")
    );
    // A conversation is kept once the process names it: after its first answer.
    client.session_send("notes", "one").unwrap();
    wait_for(&client, "scout", |s| s.pending == Some(0));

    let dev = client.session_start_with("notes", Some("dev"), false).unwrap();
    assert_eq!(dev.profile.as_deref(), Some("dev"));
    assert_ne!(dev.pid, plain.pid, "the process under no profile ended");
    let dev_id = dev.session_id.clone().unwrap();
    assert_ne!(dev_id, plain_id, "a conversation of its own");
    let args = wait_lines(&src.join("args.txt"), 1);
    assert!(
        args[0].ends_with(&format!(
            "--tools Read,Glob,Grep,Bash,Edit --allowedTools Bash(slinger *) Edit Bash(git *) \
             --add-dir {} --verbose --session-id {dev_id}",
            home.display()
        )),
        "its rules in place of the agent's, in its dir, reading the home: {}",
        args[0]
    );
    client.session_send("notes", "hi").unwrap();
    wait_for(&client, "scout", |s| s.pending == Some(0));
    let again = client.session_start_with("notes", Some("dev"), false).unwrap();
    assert_eq!(again.pid, dev.pid, "a running one under the same profile stays");

    // Each profile resumes its own conversation.
    client.session_start("notes").unwrap();
    assert!(wait_lines(&home.join("args.txt"), 2)[1].ends_with(&format!("--resume {plain_id}")));
    client.session_start_with("notes", Some("dev"), false).unwrap();
    assert!(wait_lines(&src.join("args.txt"), 2)[1].ends_with(&format!("--resume {dev_id}")));
    assert_eq!(client.session_show("scout").unwrap().profile.as_deref(), Some("dev"));
    let ids = std::fs::read_to_string(data.path().join("sessions.json")).unwrap();
    assert!(ids.contains(&format!("\"scout#dev\": \"{dev_id}\"")), "{ids}");
    assert!(ids.contains(&format!("\"scout\": \"{plain_id}\"")), "{ids}");
    daemon.stop();
}

#[test]
fn a_start_under_a_profile_that_is_not_there_is_not_found_and_ends_nothing() {
    let (data, projects) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    let daemon = Daemon::start_with_claude(data.path(), &fake_claude());
    let client = daemon.client();
    open_with_profile(&client, &projects);
    let running = client.session_start_with("notes", Some("dev"), false).unwrap();

    let err = client.session_start_with("notes", Some("nope"), false).unwrap_err();
    assert_eq!(err.code(), "not_found");
    assert!(err.hint().unwrap().contains("dev"), "{err:?}");
    let gone = dir(&projects, "gone");
    client.profile_set("notes", profile("gone", Some(&gone), &[])).unwrap();
    std::fs::remove_dir(&gone).unwrap();
    let err = client.session_start_with("notes", Some("gone"), false).unwrap_err();
    assert_eq!(err.code(), "not_found", "{err}");
    assert!(err.hint().unwrap().contains("profile set"), "{err:?}");
    let shown = client.session_show("scout").unwrap();
    assert_eq!(shown.pid, running.pid, "a refused start ends nothing");
    daemon.stop();
}

#[test]
fn a_sling_that_names_a_profile_reaches_only_a_process_under_it() {
    let (data, projects) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    let daemon = Daemon::start_with_claude(data.path(), &fake_claude());
    let client = daemon.client();
    let (_, src) = open_with_profile(&client, &projects);
    let ask = |profile: Option<&str>| SlingAsk {
        canvas: "notes".into(),
        prompt: "hi".into(),
        profile: profile.map(str::to_string),
        ..SlingAsk::default()
    };
    let slings = || {
        let filter = EventFilter { kind: Some("sling_sent".into()), ..EventFilter::default() };
        client.record_list(filter).unwrap().len()
    };

    client.session_start("notes").unwrap();
    let err = client.sling_send(ask(Some("dev"))).unwrap_err();
    assert_eq!(err.code(), "conflict", "it runs under no profile: {err}");
    assert!(err.hint().unwrap().contains("--profile dev"), "{err:?}");
    assert_eq!(client.sling_send(ask(Some("nope"))).unwrap_err().code(), "not_found");
    assert_eq!(slings(), 0, "a refused sling leaves no trace");

    client.session_start_with("notes", Some("dev"), false).unwrap();
    let slung = client.sling_send(ask(Some("dev"))).unwrap();
    assert_eq!(slung.agent, "scout");
    client.sling_send(ask(None)).unwrap();
    assert_eq!(wait_lines(&src.join("in.txt"), 2).len(), 2);
    assert_eq!(slings(), 2);
    daemon.stop();
}
