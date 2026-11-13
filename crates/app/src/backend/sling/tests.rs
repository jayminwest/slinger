//! The sling thread against a real daemon: what it sends, what it hears,
//! and the page it renders under new ink.

use super::*;
use crate::backend::testing::{delivered, fake_claude, user_stroke, wait_until, waiting_claude};
use crate::backend::tests::{open_notes, with_store};
use crate::scene::canvas::ItemId;
use crate::scene::sidebar::Status;

/// The next thing heard, waiting up to ten seconds for it.
fn wait(slinger: &Slinger, got: &mut Vec<Heard>) -> Heard {
    wait_until(|| {
        got.extend(slinger.heard());
        !got.is_empty()
    });
    got.remove(0)
}

/// The agent a sling went to; the daemon counts what it costs.
fn went(heard: Heard) -> String {
    match heard {
        Heard::Went(agent, Some(tokens)) if tokens > 0 => agent,
        other => panic!("not gone with a cost: {other:?}"),
    }
}

fn failed(heard: Heard) -> Problem {
    match heard {
        Heard::Failed(problem) => problem,
        other => panic!("not a failure: {other:?}"),
    }
}

fn ask(ids: Vec<ItemId>, prompt: &str) -> Ask {
    let (canvas, page, prompt) = ("notes".into(), "p1".into(), prompt.into());
    Ask { canvas, page, ids, prompt, profile: Profile::Running }
}

/// Slings `prompt` so that it waits for a listener and nobody listens: a
/// listener is there as it goes (else the sling starts the agent), takes it
/// in a session that then ends, and the sling waits again. Returns the
/// progress that says nobody listens.
fn sling_unheard(
    daemon: &Client,
    slinger: &Slinger,
    got: &mut Vec<Heard>,
    prompt: &str,
) -> Progress {
    assert_eq!(daemon.session_listen("notes", Duration::ZERO).unwrap().sling, None);
    slinger.send(ask(vec![ItemId(1)], prompt));
    assert_eq!(went(wait(slinger, got)), "scout");
    let mut session = std::process::Command::new("sleep").arg("60").spawn().unwrap();
    let taken = daemon.session_listen_in("notes", Duration::ZERO, Some(session.id())).unwrap();
    assert!(taken.sling.is_some(), "the listener takes it");
    session.kill().unwrap();
    session.wait().unwrap();
    loop {
        if let Heard::Progress(p) = wait(slinger, got)
            && p.unheard
        {
            return p;
        }
    }
}

#[test]
fn a_sling_reaches_the_running_agent_with_the_selection_and_the_prompt() {
    let bin = tempfile::tempdir().unwrap();
    with_store(Some(&fake_claude(bin.path())), |daemon| {
        let home = open_notes(daemon);
        user_stroke(daemon, "notes");
        let slinger = Slinger::start(Client::new("/nonexistent/x.sock"), None);
        let got = &mut Vec::new();

        slinger.send(ask(vec![ItemId(1)], "what is missing?"));
        let lost = failed(wait(&slinger, got));
        assert!(lost.hint.unwrap().starts_with("start it with '"), "no daemon");

        // No process runs: the sling starts the agent, whose process takes it.
        slinger.reconnect(daemon.clone());
        slinger.send(ask(vec![ItemId(1)], "what is missing?"));
        assert_eq!(went(wait(&slinger, got)), "scout");
        let Heard::Progress(took) = wait(&slinger, got) else { panic!("no progress") };
        assert!(took.running && !took.unheard, "{took:?}");
        let message = delivered(home.path(), "what is missing?");
        assert!(message.contains("slinger-sling 1 canvas=notes selected=c1"), "{message}");
        assert!(message.contains("what is missing?"), "{message}");

        slinger.send(Ask { page: "p9".into(), ..ask(vec![ItemId(1)], "") });
        let gone = loop {
            if let Heard::Failed(problem) = wait(&slinger, got) {
                break problem;
            }
        };
        assert!(gone.message.contains("p9"), "a page not there: {gone:?}");
        daemon.session_stop("notes").unwrap();
    });
}

#[test]
fn a_sling_is_followed_until_the_agent_answers_or_stops() {
    let bin = tempfile::tempdir().unwrap();
    with_store(Some(&waiting_claude(bin.path())), |daemon| {
        let home = open_notes(daemon);
        user_stroke(daemon, "notes");
        daemon.session_start("notes").unwrap();
        wait_until(|| daemon.session_show("scout").unwrap().context.is_some());
        let slinger = Slinger::start(daemon.clone(), None);
        let got = &mut Vec::new();

        slinger.send(ask(vec![ItemId(1)], "what is missing?"));
        assert_eq!(went(wait(&slinger, got)), "scout");
        let working = Progress {
            running: true,
            pending: 1,
            blocked: None,
            answer: None,
            profile: None,
            unheard: false,
            tokens: Some(2008),
        };
        assert_eq!(wait(&slinger, got), Heard::Progress(working), "the session's context");

        std::fs::write(home.path().join("go"), "").unwrap();
        let answer = Some("ok".to_string());
        let answered = Progress {
            running: true,
            pending: 0,
            blocked: None,
            answer,
            profile: None,
            unheard: false,
            tokens: Some(2008),
        };
        assert_eq!(wait(&slinger, got), Heard::Progress(answered));
        std::thread::sleep(3 * EVERY);
        assert_eq!(slinger.heard(), [], "no more once it answered");

        slinger.send(ask(vec![ItemId(1)], "and now?"));
        assert_eq!(went(wait(&slinger, got)), "scout");
        daemon.session_stop("notes").unwrap();
        let stopped = loop {
            if let Heard::Progress(p) = wait(&slinger, got)
                && !p.running
            {
                break p;
            }
        };
        assert!(!stopped.running, "{stopped:?}");
    });
}

#[test]
fn a_started_agent_s_pill_counts_its_context_while_older_slings_wait_for_a_listener() {
    let bin = tempfile::tempdir().unwrap();
    with_store(Some(&waiting_claude(bin.path())), |daemon| {
        let home = open_notes(daemon);
        user_stroke(daemon, "notes");
        let slinger = Slinger::start(daemon.clone(), None);
        let got = &mut Vec::new();
        sling_unheard(daemon, &slinger, got, "nobody listens");

        daemon.session_start("notes").unwrap();
        wait_until(|| daemon.session_show("scout").unwrap().context.is_some());
        slinger.send(ask(vec![ItemId(1)], "what is missing?"));
        while !matches!(wait(&slinger, got), Heard::Went(..)) {}
        // The process took it: the pill says it works, with the context its
        // process holds, not that nobody listens (slinger-1bc6f7).
        let working = Progress {
            running: true,
            pending: 1,
            blocked: None,
            answer: None,
            profile: None,
            unheard: false,
            tokens: Some(2008),
        };
        assert_eq!(wait(&slinger, got), Heard::Progress(working));
        std::fs::write(home.path().join("go"), "").unwrap();
        daemon.session_stop("notes").unwrap();
    });
}

#[test]
fn a_sling_waiting_for_a_listener_waits_through_a_process_started_since() {
    let bin = tempfile::tempdir().unwrap();
    with_store(Some(&fake_claude(bin.path())), |daemon| {
        let _home = open_notes(daemon);
        user_stroke(daemon, "notes");
        let slinger = Slinger::start(daemon.clone(), None);
        let got = &mut Vec::new();
        let waits = sling_unheard(daemon, &slinger, got, "nobody listens");
        assert!(waits.running, "{waits:?}");

        // The process takes the next sling, not this one: its pill still
        // says nobody listens, not that the agent is done (slinger-a615f8).
        daemon.session_start("notes").unwrap();
        wait_until(|| daemon.session_show("scout").unwrap().deliver == "process");
        std::thread::sleep(3 * EVERY);
        assert_eq!(slinger.heard(), [], "it waits as it did");

        let taken = daemon.session_listen("notes", Duration::from_secs(5)).unwrap();
        assert!(taken.sling.is_some(), "the sling still waits");
        let Heard::Progress(took) = wait(&slinger, got) else { panic!("no progress") };
        assert!(took.running && took.pending > 0 && !took.unheard, "{took:?}");
        daemon.session_stop("notes").unwrap();
    });
}

#[test]
fn a_sling_under_a_picked_profile_starts_the_agent_under_it_first() {
    let (bin, dev) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    with_store(Some(&fake_claude(bin.path())), |daemon| {
        let home = open_notes(daemon);
        user_stroke(daemon, "notes");
        let dir = Some(dev.path().to_string_lossy().into_owned());
        let profile = slinger_control::Profile { name: "dev".into(), dir, tools: vec![] };
        daemon.profile_set("notes", profile).unwrap();
        let slinger = Slinger::start(daemon.clone(), None);
        let got = &mut Vec::new();
        let under = |profile: Option<&str>, prompt: &str| Ask {
            profile: Profile::Picked(profile.map(str::to_string)),
            ..ask(vec![ItemId(1)], prompt)
        };
        let running = || daemon.session_show("scout").unwrap().profile;

        slinger.send(under(Some("dev"), "not running yet"));
        assert_eq!(went(wait(&slinger, got)), "scout");
        delivered(dev.path(), "not running yet");
        assert_eq!(running(), Some("dev".into()));
        let agent = crate::backend::workspaces(daemon).unwrap()[0].agent.clone().unwrap();
        assert_eq!((agent.profile, agent.profiles), (Some("dev".into()), vec!["dev".into()]));

        slinger.send(under(None, "back to its own"));
        while !matches!(wait(&slinger, got), Heard::Went(..)) {}
        delivered(home.path(), "back to its own");
        assert_eq!(running(), None, "restarted within its own limits");

        slinger.send(under(Some("gone"), "nowhere"));
        let gone = loop {
            if let Heard::Failed(problem) = wait(&slinger, got) {
                break problem;
            }
        };
        assert!(gone.hint.unwrap_or_default().contains("dev"), "{}", gone.message);
        assert_eq!(running(), None, "the running process stays");
        daemon.session_stop("notes").unwrap();
    });
}

/// A [`Render`] that paints the area solid grey and says one word at
/// the stroke `user_stroke` draws.
fn grey(area: &BackdropArea, _: &Path, image: &Path, words: &Path) -> bool {
    let size = area.width as usize * area.height as usize * 3;
    let word = r#"[{"text":"circled","x":1000,"y":1000,"w":100,"h":100}]"#;
    fs::write(image, vec![128; size]).is_ok() && fs::write(words, word).is_ok()
}

#[test]
fn a_sling_over_a_pdf_sends_the_page_it_rendered_and_its_words() {
    let bin = tempfile::tempdir().unwrap();
    with_store(Some(&fake_claude(bin.path())), |daemon| {
        let _home = open_notes(daemon);
        let file = bin.path().join("paper.pdf");
        fs::write(&file, "%PDF-1.4 a stand-in").unwrap();
        let hash = daemon.blob_put(&file).unwrap();
        let pdf = serde_json::json!({ "op": "add", "item": { "page": "p1", "kind": "pdf",
            "x": 0, "y": 0, "w": 16000, "h": 20706, "hash": hash, "pdfpage": 1,
            "pagesize": [6120, 7920] } });
        let (canvas, by) = ("notes".to_string(), "user".to_string());
        daemon
            .canvas_apply(slinger_control::CanvasOps { canvas, by, ops: pdf, once: None })
            .unwrap();
        user_stroke(daemon, "notes");
        // A cache of its own: the app's is shared by every test process and
        // worktree, and a fetched file is named by its bytes (slinger-f19088).
        let cache = tempfile::tempdir().unwrap();
        let slinger = Slinger::start(daemon.clone(), Some((grey, cache.path().to_owned())));
        let got = &mut Vec::new();
        slinger.send(ask(vec![], "what does it say?"));
        assert_eq!(went(wait(&slinger, got)), "scout");
        let filter =
            slinger_control::EventFilter { kind: Some("sling_sent".into()), ..Default::default() };
        let sent = daemon.record_list(filter).unwrap();
        let packet = sent[0]["payload"]["packet"].as_str().unwrap().to_string();
        let text = bin.path().join("packet.txt");
        daemon.blob_get(&packet, &text).unwrap();
        let text = fs::read_to_string(text).unwrap();
        assert!(text.contains("over=d1 img=c2.png \"circled\"\n"), "{text}");
        let ours = format!("slinger-app-sling-{}-", std::process::id());
        let left = fs::read_dir(std::env::temp_dir()).unwrap().flatten().filter(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            name.starts_with(&ours) && e.path().join("0.rgb").exists()
        });
        assert_eq!(left.count(), 0, "the files are gone after the reply");
        let fetched = cache.path().join(format!("{}.pdf", &hash["sha256:".len()..]));
        assert!(fetched.is_file(), "the PDF came into the cache it was given");
    });
}

/// The status the sidebar shows for scout, and its note.
fn shown(d: &Client) -> (Option<Status>, Option<String>) {
    let list = crate::backend::workspaces(d).unwrap();
    let agent = list[0].agent.clone().unwrap();
    (agent.status, agent.note)
}

#[test]
fn the_sidebar_s_status_follows_the_daemon_s_count_of_pending_messages() {
    let bin = tempfile::tempdir().unwrap();
    with_store(Some(&waiting_claude(bin.path())), |d| {
        let home = open_notes(d);
        d.status_set("scout", "done", None).unwrap();
        assert_eq!(shown(d), (Some(Status::Done), None), "never started: its hooks say");
        d.session_start("notes").unwrap();
        assert_eq!(shown(d), (Some(Status::Done), None), "it said so");
        let ask = SlingAsk {
            canvas: "notes".into(),
            page: None,
            select: vec![],
            prompt: "hi".into(),
            profile: None,
            backdrops: vec![],
        };
        d.sling_send(ask).unwrap();
        assert_eq!(shown(d), (Some(Status::Working), None), "one pending, no hook yet");
        d.status_set("scout", "blocked", Some("which key?")).unwrap();
        assert_eq!(shown(d), (Some(Status::Blocked), Some("which key?".into())));
        std::fs::write(home.path().join("go"), "").unwrap();
        wait_until(|| shown(d).0 == Some(Status::Done));
        d.session_stop("notes").unwrap();
        assert_eq!(shown(d).0, Some(Status::Stopped), "stopped, whatever it said");
    });
}
