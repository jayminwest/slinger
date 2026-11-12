//! Sending a sling (docs/service.md, "Slings"): a thread of its own sends
//! each [`Ask`] as one `sling_send` and hands back what the daemon said, so
//! the main thread never waits on the socket. The daemon picks the agent
//! (the agent of the open workspace whose canvas it is) and replies once
//! the message is in the agent's process; the agent's answer comes later,
//! on the canvas.

use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;

use slinger_control::{Client, SlingAsk};

use crate::backend::Problem;
use crate::mirror;
use crate::scene::sling::Ask;

/// The thread that sends slings. Dropping it ends the thread once the
/// sling in flight is answered.
#[derive(Debug)]
pub struct Slinger {
    daemon: Arc<Mutex<Client>>,
    asks: Sender<Ask>,
    answers: Receiver<Result<(), Problem>>,
}

impl Slinger {
    /// Starts sending through `daemon`. Returns at once.
    pub fn start(daemon: Client) -> Self {
        let daemon = Arc::new(Mutex::new(daemon));
        let (asks, to_send) = mpsc::channel::<Ask>();
        let (answer, answers) = mpsc::channel();
        let theirs = Arc::clone(&daemon);
        thread::spawn(move || {
            for ask in to_send {
                let client = theirs.lock().unwrap_or_else(PoisonError::into_inner).clone();
                if answer.send(sling(&client, ask)).is_err() {
                    return;
                }
            }
        });
        Self { daemon, asks, answers }
    }

    /// Sends through `daemon` from the next sling on.
    pub fn reconnect(&self, daemon: Client) {
        *self.daemon.lock().unwrap_or_else(PoisonError::into_inner) = daemon;
    }

    /// Hands `ask` to the thread. Never waits.
    pub fn send(&self, ask: Ask) {
        // The thread lives as long as the slinger, so the send cannot fail.
        let _ = self.asks.send(ask);
    }

    /// The answers since the last call, oldest first. Never waits.
    pub fn answers(&self) -> Vec<Result<(), Problem>> {
        self.answers.try_iter().collect()
    }
}

/// Sends `ask` as one `sling_send`: the canvas, the page, the selected
/// items by the canvas file's ids, and the prompt.
fn sling(daemon: &Client, ask: Ask) -> Result<(), Problem> {
    let select = ask.ids.into_iter().map(mirror::item_ref).collect();
    let request = SlingAsk { canvas: ask.canvas, page: Some(ask.page), select, prompt: ask.prompt };
    daemon.sling_send(request).map(drop).map_err(Problem::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::testing::{fake_claude, user_stroke, wait_until};
    use crate::backend::tests::{open_notes, with_store};
    use crate::scene::canvas::ItemId;

    /// The next answer, waiting up to ten seconds for it.
    fn wait(slinger: &Slinger) -> Result<(), Problem> {
        let mut got = None;
        wait_until(|| {
            got = slinger.answers().pop();
            got.is_some()
        });
        got.unwrap()
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

            slinger.send(ask(vec![ItemId(1)], "what is missing?"));
            let lost = wait(&slinger).unwrap_err();
            assert!(lost.hint.unwrap().starts_with("start it with '"), "no daemon");

            slinger.reconnect(daemon.clone());
            slinger.send(ask(vec![ItemId(1)], "what is missing?"));
            let idle = wait(&slinger).unwrap_err();
            assert!(idle.hint.unwrap().contains("session start"), "{}", idle.message);

            daemon.session_start("notes").unwrap();
            slinger.send(ask(vec![ItemId(1)], "what is missing?"));
            assert_eq!(wait(&slinger), Ok(()));
            let file = home.path().join("in.txt");
            let mut message = String::new();
            wait_until(|| {
                message = std::fs::read_to_string(&file).unwrap_or_default();
                message.contains("what is missing?")
            });
            assert!(message.contains("slinger-sling 1 canvas=notes selected=c1"), "{message}");
            assert!(message.contains("what is missing?"), "{message}");

            slinger.send(Ask { page: "p9".into(), ..ask(vec![ItemId(1)], "") });
            let gone = wait(&slinger).unwrap_err();
            assert!(gone.message.contains("p9"), "a page not there: {gone:?}");
            daemon.session_stop("notes").unwrap();
        });
    }
}
