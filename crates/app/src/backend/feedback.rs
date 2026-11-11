//! Saving feedback (docs/service.md, "Feedback"): each [`Note`] goes as one
//! `feedback_add` on a thread of its own, and the daemon's answer comes
//! back to the main thread, which never waits on the socket.

use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

use slinger_control::{Client, FeedbackNote};

use crate::backend::Problem;
use crate::scene::feedback::Note;

/// Sends notes and hands back what the daemon said to each.
#[derive(Debug)]
pub struct Noter {
    daemon: Client,
    answer: Sender<Result<(), Problem>>,
    answers: Receiver<Result<(), Problem>>,
}

impl Noter {
    /// Sends through `daemon`. Starts no thread until a note goes.
    pub fn new(daemon: Client) -> Self {
        let (answer, answers) = mpsc::channel();
        Self { daemon, answer, answers }
    }

    /// Sends through `daemon` from the next note on.
    pub fn reconnect(&mut self, daemon: Client) {
        self.daemon = daemon;
    }

    /// Hands `note` to a thread that sends it. Never waits.
    pub fn send(&self, note: Note) {
        let (daemon, answer) = (self.daemon.clone(), self.answer.clone());
        thread::spawn(move || {
            // The noter is gone, and the answer with it: nobody waits for it.
            let _ = answer.send(add(&daemon, note));
        });
    }

    /// The answers since the last call, oldest first. Never waits.
    pub fn answers(&self) -> Vec<Result<(), Problem>> {
        self.answers.try_iter().collect()
    }
}

/// Sends `note` as one `feedback_add`, with each part of its context the
/// window had.
fn add(daemon: &Client, note: Note) -> Result<(), Problem> {
    let Note { text, context } = note;
    let selected = Some(context.selected as u64).filter(|&n| n > 0);
    let (workspace, canvas, page, tool) =
        (context.workspace, context.canvas, context.page, context.tool);
    let note = FeedbackNote { text, workspace, canvas, page, tool, selected };
    daemon.feedback_add(note).map(drop).map_err(Problem::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::testing::wait_until;
    use crate::backend::tests::with_store;
    use crate::scene::feedback::Context;

    fn note(text: &str) -> Note {
        let context = Context { tool: Some("pen".into()), selected: 1, ..Context::default() };
        Note { text: text.into(), context }
    }

    /// The next answer, waiting for it.
    fn wait(noter: &Noter) -> Result<(), Problem> {
        let mut got = Vec::new();
        wait_until(|| {
            got.extend(noter.answers());
            !got.is_empty()
        });
        got.remove(0)
    }

    #[test]
    fn a_note_reaches_the_daemon_with_its_context_or_says_why_not() {
        with_store(None, |daemon| {
            let noter = Noter::new(daemon.clone());
            noter.send(note("the pen lags"));
            assert_eq!(wait(&noter), Ok(()));
            let saved = daemon.feedback_list(false, None).unwrap().entries;
            let first = &saved[0];
            assert_eq!((first.text.as_str(), first.tool.as_deref()), ("the pen lags", Some("pen")));
            assert_eq!((first.selected, first.canvas.as_deref()), (Some(1), None));

            noter.send(note(" "));
            assert!(wait(&noter).unwrap_err().message.contains("empty"));
        });
        let mut gone = Noter::new(Client::new("/nonexistent/x.sock"));
        gone.reconnect(Client::new("/nonexistent/y.sock"));
        gone.send(note("x"));
        assert!(wait(&gone).unwrap_err().message.contains("y.sock"), "the new socket");
    }
}
