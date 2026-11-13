//! Saving feedback (docs/service.md, "Feedback"): each [`Note`] goes as one
//! `feedback_add` on a thread of its own, and the daemon's answer comes
//! back to the main thread, which never waits on the socket. The recent
//! open notes (or all of them, resolved ones too) come the same way, from
//! one `feedback_list`, and a note resolves the same way, from one
//! `feedback_resolve`, the list following.
//! Where the notes go comes at once, from one `feedback_path`
//! ([`feedback_file`]).

use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

use slinger_control::{Client, FeedbackEntry, FeedbackNote};

use crate::backend::Problem;
use crate::scene::feedback::{LISTED, Note, Recent};

/// Sends notes and hands back what the daemon said to each, and lists the
/// recent ones.
#[derive(Debug)]
pub struct Noter {
    daemon: Client,
    answer: Sender<Result<(), Problem>>,
    answers: Receiver<Result<(), Problem>>,
    list: Sender<Vec<Recent>>,
    lists: Receiver<Vec<Recent>>,
    resolution: Sender<Result<(), Problem>>,
    resolutions: Receiver<Result<(), Problem>>,
}

impl Noter {
    /// Sends through `daemon`. Starts no thread until a note goes.
    pub fn new(daemon: Client) -> Self {
        let (answer, answers) = mpsc::channel();
        let (list, lists) = mpsc::channel();
        let (resolution, resolutions) = mpsc::channel();
        Self { daemon, answer, answers, list, lists, resolution, resolutions }
    }

    /// Sends through `daemon` from the next note on.
    pub fn reconnect(&mut self, daemon: Client) {
        self.daemon = daemon;
    }

    /// Hands `note` to a thread that sends it. Never waits.
    pub fn send(&self, note: Note) {
        self.spawn(Job::Add(note));
    }

    /// The answers since the last call, oldest first. Never waits.
    pub fn answers(&self) -> Vec<Result<(), Problem>> {
        self.answers.try_iter().collect()
    }

    /// Asks a thread for the most recent open notes, or with `all` the
    /// most recent notes, resolved ones too. Never waits. A daemon that
    /// does not answer lists nothing: saving a note says why.
    pub fn list(&self, all: bool) {
        self.spawn(Job::List(all));
    }

    /// Hands the note named `ts` to a thread that resolves it, then lists
    /// the recent notes again, as [`Noter::list`] with `all`. Never waits.
    pub fn resolve(&self, ts: String, all: bool) {
        self.spawn(Job::Resolve(ts, all));
    }

    /// Hands `job` to a thread of its own, which sends its answers back.
    fn spawn(&self, job: Job) {
        let (daemon, answer) = (self.daemon.clone(), self.answer.clone());
        let (list, resolution) = (self.list.clone(), self.resolution.clone());
        thread::spawn(move || {
            // The noter is gone, and the answers with it: nobody waits for them.
            let all = match job {
                Job::Add(note) => {
                    let _ = answer.send(add(&daemon, note));
                    return;
                }
                Job::List(all) => all,
                Job::Resolve(ts, all) => {
                    let resolved = daemon.feedback_resolve(&ts, None);
                    let _ = resolution.send(resolved.map(drop).map_err(Problem::from));
                    all
                }
            };
            let limit = Some(LISTED as u64);
            let Ok(listed) = daemon.feedback_list(all, limit) else { return };
            let _ = list.send(listed.entries.into_iter().map(recent).collect());
        });
    }

    /// The answers to the notes resolved since the last call, oldest
    /// first. Never waits.
    pub fn resolutions(&self) -> Vec<Result<(), Problem>> {
        self.resolutions.try_iter().collect()
    }

    /// The newest list since the last call, if one came. Never waits.
    pub fn listed(&self) -> Option<Vec<Recent>> {
        self.lists.try_iter().last()
    }
}

/// What a [`Noter`]'s thread does: save a note, list the recent ones (all
/// of them, or the open ones), or resolve one by its `ts` and list them
/// again.
enum Job {
    Add(Note),
    List(bool),
    Resolve(String, bool),
}

/// A listed note as the entry lists it: when it was saved, and how it was
/// resolved, if it was: `resolved 11-12 09:30`, then `: ` and the note.
fn recent(entry: FeedbackEntry) -> Recent {
    let resolved = entry.resolved.map(|r| match r.note {
        Some(note) => format!("resolved {}: {note}", when(&r.ts)),
        None => format!("resolved {}", when(&r.ts)),
    });
    Recent { when: when(&entry.ts), ts: entry.ts, text: entry.text, resolved }
}

/// The daemon's feedback file, for the Finder to show
/// (`show-feedback-file`), from one `feedback_path`; else what the flash
/// says: why the daemon did not answer, or, before the first note, where
/// notes will go.
pub fn feedback_file(daemon: &Client) -> Result<String, String> {
    let file = daemon.feedback_path().map_err(|e| Problem::from(e).message)?;
    let shown = file.to_string_lossy().into_owned();
    if file.exists() { Ok(shown) } else { Err(format!("No feedback yet: notes go to {shown}")) }
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
            noter.list(false);
            assert_eq!(listed(&noter), []);
            for text in ["one", "two", "three", "four", "five", "six"] {
                noter.send(note(text));
                assert_eq!(wait(&noter), Ok(()));
            }
            noter.list(false);
            let got = listed(&noter);
            let texts: Vec<_> = got.iter().map(|r| r.text.as_str()).collect();
            assert_eq!(texts, ["six", "five", "four", "three", "two", "one"]);
            let when = &got[0].when;
            assert!(when.len() == 11 && when.as_bytes()[2] == b'-', "{when}");
        });
    }

    #[test]
    fn a_note_resolves_by_its_ts_then_leaves_the_list_or_says_why_not() {
        with_store(None, |daemon| {
            let noter = Noter::new(daemon.clone());
            for text in ["one", "two"] {
                noter.send(note(text));
                assert_eq!(wait(&noter), Ok(()));
            }
            noter.list(false);
            let two = listed(&noter).remove(0);
            noter.resolve(two.ts.clone(), false);
            let mut got = Vec::new();
            wait_until(|| {
                got.extend(noter.resolutions());
                !got.is_empty()
            });
            assert_eq!(got, [Ok(())]);
            let after = listed(&noter);
            assert_eq!(after.iter().map(|r| r.text.as_str()).collect::<Vec<_>>(), ["one"]);

            noter.resolve(two.ts, false);
            let mut got = Vec::new();
            wait_until(|| {
                got.extend(noter.resolutions());
                !got.is_empty()
            });
            assert!(got[0].is_err(), "resolved already: {got:?}");
        });
    }

    #[test]
    fn the_feedback_file_is_where_notes_go_once_there_is_one_or_the_flash_says_why_not() {
        with_store(None, |daemon| {
            let before = feedback_file(daemon).unwrap_err();
            assert!(before.starts_with("No feedback yet: notes go to /"), "{before}");
            let noter = Noter::new(daemon.clone());
            noter.send(note("the pen lags"));
            assert_eq!(wait(&noter), Ok(()));
            let file = feedback_file(daemon).unwrap();
            assert!(before.ends_with(&file), "the same file: {file}");
            let listed = daemon.feedback_list(false, None).unwrap().path;
            assert_eq!(std::path::Path::new(&file), listed);
        });
        let gone = feedback_file(&Client::new("/nonexistent/x.sock")).unwrap_err();
        assert!(gone.contains("x.sock"), "{gone}");
    }

    #[test]
    fn all_lists_the_resolved_notes_too_with_how_they_were_resolved() {
        with_store(None, |daemon| {
            let noter = Noter::new(daemon.clone());
            for text in ["one", "two", "three"] {
                noter.send(note(text));
                assert_eq!(wait(&noter), Ok(()));
            }
            let saved = daemon.feedback_list(false, None).unwrap().entries;
            daemon.feedback_resolve(&saved[2].ts, Some("slinger-1234".into())).unwrap();
            noter.resolve(saved[1].ts.clone(), true);
            let all = listed(&noter);
            let texts: Vec<_> = all.iter().map(|r| r.text.as_str()).collect();
            assert_eq!(texts, ["three", "two", "one"], "newest first, resolved or not");
            assert_eq!(all[0].resolved, None, "open");
            let two = all[1].resolved.as_deref().unwrap();
            assert!(two.starts_with("resolved ") && two.len() == 20, "{two}");
            let one = all[2].resolved.as_deref().unwrap();
            assert!(one.starts_with("resolved ") && one.ends_with(": slinger-1234"), "{one}");

            noter.list(false);
            let open = listed(&noter);
            assert_eq!(open.iter().map(|r| r.text.as_str()).collect::<Vec<_>>(), ["three"]);
        });
    }

    #[test]
    fn when_shows_the_day_and_minute_or_a_ts_of_another_shape_whole() {
        assert_eq!(when("2026-11-11T14:02:09-08:00"), "11-11 14:02");
        assert_eq!(when("2026-11-11T14:02:09.123-08:00"), "11-11 14:02");
        assert_eq!(when("yesterday"), "yesterday");
    }
}
