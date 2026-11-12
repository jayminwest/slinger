//! Saving feedback (docs/service.md, "Feedback"): each [`Note`] goes as one
//! `feedback_add` on a thread of its own, and the daemon's answer comes
//! back to the main thread, which never waits on the socket. The recent
//! open notes come the same way, from one `feedback_list`.

use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

use slinger_control::{Client, FeedbackNote};

use crate::backend::Problem;
use crate::scene::feedback::{Note, RECENT, Recent};

/// Sends notes and hands back what the daemon said to each, and lists the
/// recent ones.
#[derive(Debug)]
pub struct Noter {
    daemon: Client,
    answer: Sender<Result<(), Problem>>,
    answers: Receiver<Result<(), Problem>>,
    list: Sender<Vec<Recent>>,
    lists: Receiver<Vec<Recent>>,
}

impl Noter {
    /// Sends through `daemon`. Starts no thread until a note goes.
    pub fn new(daemon: Client) -> Self {
        let (answer, answers) = mpsc::channel();
        let (list, lists) = mpsc::channel();
        Self { daemon, answer, answers, list, lists }
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

    /// Asks a thread for the most recent open notes. Never waits. A daemon
    /// that does not answer lists nothing: saving a note says why.
    pub fn list(&self) {
        let (daemon, list) = (self.daemon.clone(), self.list.clone());
        thread::spawn(move || {
            let limit = Some(RECENT as u64);
            let Ok(listed) = daemon.feedback_list(false, limit) else { return };
            // As in `send`: nobody waits for it.
            let recent = listed.entries.into_iter();
            let _ = list.send(recent.map(|e| Recent { when: when(&e.ts), text: e.text }).collect());
        });
    }

    /// The newest list since the last call, if one came. Never waits.
    pub fn listed(&self) -> Option<Vec<Recent>> {
        self.lists.try_iter().last()
    }
}

/// When a note was saved, as the entry lists it: `11-11 14:02` from its
/// `ts` (`2026-11-11T14:02:09-08:00`), or the whole `ts` when it is not
/// that shape.
fn when(ts: &str) -> String {
    match (ts.get(5..10), ts.get(11..16)) {
        (Some(day), Some(time)) if ts.as_bytes().get(10) == Some(&b'T') => {
            format!("{day} {time}")
        }
        _ => ts.to_string(),
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

    /// The next list, waiting for it.
    fn listed(noter: &Noter) -> Vec<Recent> {
        let mut got = None;
        wait_until(|| {
            got = noter.listed();
            got.is_some()
        });
        got.unwrap()
    }

    #[test]
    fn the_recent_open_notes_list_newest_first_with_when() {
        with_store(None, |daemon| {
            let noter = Noter::new(daemon.clone());
            noter.list();
            assert_eq!(listed(&noter), []);
            for text in ["one", "two", "three", "four", "five", "six"] {
                noter.send(note(text));
                assert_eq!(wait(&noter), Ok(()));
            }
            noter.list();
            let got = listed(&noter);
            let texts: Vec<_> = got.iter().map(|r| r.text.as_str()).collect();
            assert_eq!(texts, ["six", "five", "four", "three", "two"]);
            let when = &got[0].when;
            assert!(when.len() == 11 && when.as_bytes()[2] == b'-', "{when}");
        });
    }

    #[test]
    fn when_shows_the_day_and_minute_or_a_ts_of_another_shape_whole() {
        assert_eq!(when("2026-11-11T14:02:09-08:00"), "11-11 14:02");
        assert_eq!(when("2026-11-11T14:02:09.123-08:00"), "11-11 14:02");
        assert_eq!(when("yesterday"), "yesterday");
    }
}
