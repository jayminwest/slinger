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
use crate::scene::sling::Ask;
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
    loop {
        match asks.recv_timeout(EVERY) {
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
/// items by the canvas file's ids, and the prompt. Returns the agent it
/// went to and the tokens it costs.
fn sling(daemon: &Client, ask: Ask) -> Result<(String, Option<u64>), Problem> {
    let select = ask.ids.into_iter().map(mirror::item_ref).collect();
    let (canvas, page, prompt) = (ask.canvas, Some(ask.page), ask.prompt);
    let request = SlingAsk { canvas, page, select, prompt, profile: None };
    let slung = daemon.sling_send(request).map_err(Problem::from)?;
    Ok((slung.agent, slung.tokens))
}

/// What the window wants of the agent's `session` while a sling is out.
fn progress(session: &Session) -> Progress {
    let blocked = session.status.as_deref() == Some("blocked");
    Progress {
        running: session.process.as_deref() == Some("running"),
        pending: session.pending.unwrap_or(0),
        blocked: blocked.then(|| session.note.clone().unwrap_or_default()),
        answer: session.answer.clone(),
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
        Ask { canvas: "notes".into(), page: "p1".into(), ids, prompt: prompt.into() }
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
            let idle = failed(wait(&slinger, got));
            assert!(idle.hint.unwrap().contains("session start"), "{}", idle.message);

            daemon.session_start("notes").unwrap();
            slinger.send(ask(vec![ItemId(1)], "what is missing?"));
            assert_eq!(went(wait(&slinger, got)), "scout");
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
            let working = Progress { running: true, pending: 1, blocked: None, answer: None };
            assert_eq!(wait(&slinger, got), Heard::Progress(working));

            std::fs::write(home.path().join("go"), "").unwrap();
            let answer = Some("ok".to_string());
            let answered = Progress { running: true, pending: 0, blocked: None, answer };
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
