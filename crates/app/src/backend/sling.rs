//! Sending a sling (docs/service.md, "Slings"): a thread of its own sends
//! each [`Ask`] as one `sling_send` and hands back what the daemon said, so
//! the main thread never waits on the socket. The daemon picks the agent
//! (the agent of the open workspace whose canvas it is) and replies once
//! the message is in the agent's process. The thread then follows the
//! agent's session (`session_show`, every [`EVERY`]) until the agent has
//! no message pending or its process is gone, handing back each change, so
//! the window can say it works and when it answered (SPEC P6).

use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;
use std::time::Duration;

use slinger_control::{Client, Session, SlingAsk};

use crate::backend::Problem;
use crate::mirror;
use crate::scene::sling::{Ask, Profile};
use crate::scene::slung::Progress;

/// How often the agent's session is read while a sling is out.
const EVERY: Duration = Duration::from_millis(500);

/// What the thread hands back about the sling it sent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Heard {
    /// The sling is in this agent's process, and about how many tokens the
    /// agent reads for it (`None`: the daemon does not count).
    Went(String, Option<u64>),
    /// The sling did not go.
    Failed(Problem),
    /// The agent's session after the sling went, when it changed.
    Progress(Progress),
    /// The daemon could not be read while the agent worked.
    Lost(Problem),
}

/// The thread that sends slings. Dropping it ends the thread once the
/// sling in flight is answered.
#[derive(Debug)]
pub struct Slinger {
    daemon: Arc<Mutex<Client>>,
    asks: Sender<Ask>,
    heard: Receiver<Heard>,
}

impl Slinger {
    /// Starts sending through `daemon`. Returns at once.
    pub fn start(daemon: Client) -> Self {
        let daemon = Arc::new(Mutex::new(daemon));
        let (asks, to_send) = mpsc::channel::<Ask>();
        let (tell, heard) = mpsc::channel();
        let theirs = Arc::clone(&daemon);
        thread::spawn(move || {
            let client = || theirs.lock().unwrap_or_else(PoisonError::into_inner).clone();
            let mut next = to_send.recv().ok();
            while let Some(ask) = next.take() {
                let (agent, tokens) = match sling(&client(), ask) {
                    Ok(went) => went,
                    Err(problem) => {
                        if tell.send(Heard::Failed(problem)).is_err() {
                            return;
                        }
                        next = to_send.recv().ok();
                        continue;
                    }
                };
                if tell.send(Heard::Went(agent.clone(), tokens)).is_err() {
                    return;
                }
                next = match follow(&client, &agent, &to_send, &tell) {
                    Some(Followed::Over) => to_send.recv().ok(),
                    Some(Followed::Next(ask)) => Some(ask),
                    None => return,
                };
            }
        });
        Self { daemon, asks, heard }
    }

    /// Sends through `daemon` from the next sling on.
    pub fn reconnect(&self, daemon: Client) {
        *self.daemon.lock().unwrap_or_else(PoisonError::into_inner) = daemon;
    }

    /// Hands `ask` to the thread. Never waits. The sling followed until
    /// now is no longer followed.
    pub fn send(&self, ask: Ask) {
        // The thread lives as long as the slinger, so the send cannot fail.
        let _ = self.asks.send(ask);
    }

    /// What the thread heard since the last call, oldest first. Never
    /// waits.
    pub fn heard(&self) -> Vec<Heard> {
        self.heard.try_iter().collect()
    }
}

/// How following an agent ended.
enum Followed {
    /// It answered, its process ended, or the daemon could not be read.
    Over,
    /// A new sling came first.
    Next(Ask),
}

/// Follows `agent`'s session through the daemon `client` gives, telling
/// each change to `tell`, until it is over or a new ask comes on `asks`.
/// `None`: the slinger was dropped.
fn follow(
    client: &impl Fn() -> Client,
    agent: &str,
    asks: &Receiver<Ask>,
    tell: &Sender<Heard>,
) -> Option<Followed> {
    let mut last = None;
    // The first look is at once: a sling nobody listens for says so from the start.
    let mut pause = Duration::ZERO;
    loop {
        let waited = asks.recv_timeout(pause);
        pause = EVERY;
        match waited {
            Ok(ask) => return Some(Followed::Next(ask)),
            Err(RecvTimeoutError::Disconnected) => return None,
            Err(RecvTimeoutError::Timeout) => {}
        }
        let (heard, over) = match client().session_show(agent) {
            Ok(session) => {
                let now = progress(&session);
                let over = !now.running || now.pending == 0;
                if last.as_ref() == Some(&now) {
                    continue;
                }
                last = Some(now.clone());
                (Heard::Progress(now), over)
            }
            Err(e) => (Heard::Lost(Problem::from(e)), true),
        };
        tell.send(heard).ok()?;
        if over {
            return Some(Followed::Over);
        }
    }
}

/// Sends `ask` as one `sling_send`: the canvas, the page, the selected
/// items by the canvas file's ids, and the prompt. Under a picked profile
/// it first starts the agent of the canvas's workspace under it (the
/// daemon leaves a process running under it as it is, and restarts one
/// under another), and the sling goes only to that profile. Returns the
/// agent it went to and the tokens it costs.
fn sling(daemon: &Client, ask: Ask) -> Result<(String, Option<u64>), Problem> {
    let profile = match ask.profile {
        Profile::Running => None,
        Profile::Picked(profile) => {
            let open = daemon.workspace_list()?;
            let workspace = open.iter().find(|w| w.canvas == ask.canvas);
            // A canvas no open workspace has: the sling says so itself.
            if let Some(workspace) = workspace {
                daemon.session_start_with(&workspace.name, profile.as_deref(), false)?;
            }
            profile
        }
    };
    let select = ask.ids.into_iter().map(mirror::item_ref).collect();
    let (canvas, page, prompt) = (ask.canvas, Some(ask.page), ask.prompt);
    let request = SlingAsk { canvas, page, select, prompt, profile };
    let slung = daemon.sling_send(request).map_err(Problem::from)?;
    Ok((slung.agent, slung.tokens))
}

/// What the window wants of the agent's `session` while a sling is out.
fn progress(session: &Session) -> Progress {
    let blocked = session.status.as_deref() == Some("blocked");
    // A sling a listener takes has no process to end: it is out until taken and answered.
    let listen = session.deliver == "listen";
    let pending = session.pending.unwrap_or(0);
    Progress {
        running: listen || session.process.as_deref() == Some("running"),
        pending,
        unheard: listen && pending > 0 && session.listening == Some(false),
        blocked: blocked.then(|| session.note.clone().unwrap_or_default()),
        answer: session.answer.clone(),
        profile: session.profile.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::testing::{
        delivered, fake_claude, user_stroke, wait_until, waiting_claude,
    };
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

    #[test]
    fn a_sling_reaches_the_running_agent_with_the_selection_and_the_prompt() {
        let bin = tempfile::tempdir().unwrap();
        with_store(Some(&fake_claude(bin.path())), |daemon| {
            let home = open_notes(daemon);
            user_stroke(daemon, "notes");
            let slinger = Slinger::start(Client::new("/nonexistent/x.sock"));
            let got = &mut Vec::new();

            slinger.send(ask(vec![ItemId(1)], "what is missing?"));
            let lost = failed(wait(&slinger, got));
            assert!(lost.hint.unwrap().starts_with("start it with '"), "no daemon");

            slinger.reconnect(daemon.clone());
            slinger.send(ask(vec![ItemId(1)], "what is missing?"));
            assert_eq!(went(wait(&slinger, got)), "scout");
            let Heard::Progress(waits) = wait(&slinger, got) else { panic!("no progress") };
            assert!(waits.running && waits.unheard, "no process: it waits for a listener");

            daemon.session_start("notes").unwrap();
            slinger.send(ask(vec![ItemId(1)], "what is missing?"));
            let to = loop {
                if let Heard::Went(agent, _) = wait(&slinger, got) {
                    break agent;
                }
            };
            assert_eq!(to, "scout");
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
            let slinger = Slinger::start(daemon.clone());
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
            };
            assert_eq!(wait(&slinger, got), Heard::Progress(working));

            std::fs::write(home.path().join("go"), "").unwrap();
            let answer = Some("ok".to_string());
            let answered = Progress {
                running: true,
                pending: 0,
                blocked: None,
                answer,
                profile: None,
                unheard: false,
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
    fn a_sling_under_a_picked_profile_starts_the_agent_under_it_first() {
        let (bin, dev) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        with_store(Some(&fake_claude(bin.path())), |daemon| {
            let home = open_notes(daemon);
            user_stroke(daemon, "notes");
            let dir = Some(dev.path().to_string_lossy().into_owned());
            let profile = slinger_control::Profile { name: "dev".into(), dir, tools: vec![] };
            daemon.profile_set("notes", profile).unwrap();
            let slinger = Slinger::start(daemon.clone());
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
}
