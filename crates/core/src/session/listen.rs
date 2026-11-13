//! Listen delivery (SPEC section 5): a session nobody started for the agent
//! takes the agent's slings by asking for them. A plain Claude Code session
//! does, so the agent works with that session's own tools, settings and
//! permissions.
//!
//! One [`Listener`] per agent: the slings that wait for it, oldest first,
//! and what is known of who listens. Nothing here waits or reads a clock:
//! the server holds the listener, hands in the time and does the waiting.
//!
//! A sling waits here whether or not anyone listens, so none is refused for
//! want of a listener: the next one to listen takes it. [`Listener::attached`]
//! says whether someone is there, which is what the user is shown.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// How long after it last asked a listener still counts as there: the gap
/// between one ask ending and the next beginning.
pub const GRACE: Duration = Duration::from_secs(10);

/// One sling as a listener takes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Heard {
    /// The number of its `sling_sent` event.
    pub event: u64,
    /// The canvas it was slung on.
    pub canvas: String,
    /// The directory that holds the packet's images.
    pub dir: PathBuf,
    /// The sling's message as text ([`crate::sling::Message::text`]).
    pub text: String,
}

/// The slings waiting for one agent's listener, and whether one is there.
#[derive(Debug, Default)]
pub struct Listener {
    waiting: VecDeque<Heard>,
    /// How many asks are held open right now.
    asking: u32,
    /// When an ask last ended, or the listener last said it was done.
    seen: Option<Instant>,
    /// The sling it took and has not said it dealt with.
    working: Option<Work>,
}

/// A sling a listener works on, and the process that works on it.
#[derive(Debug)]
struct Work {
    sling: Heard,
    by: Option<u32>,
}

impl Listener {
    /// Adds a sling, behind those that wait.
    pub fn push(&mut self, sling: Heard) {
        self.waiting.push_back(sling);
    }

    /// An ask begins: it is held open until [`Listener::left`].
    pub fn asked(&mut self) {
        self.asking += 1;
    }

    /// An ask ended at `now`.
    pub fn left(&mut self, now: Instant) {
        self.asking = self.asking.saturating_sub(1);
        self.seen = Some(now);
    }

    /// The oldest sling that waits, taken by the listener that asks, which
    /// works on it in the process `by` when it names one. Asking says its
    /// last sling is dealt with, so it works only if it gets one.
    pub fn take(&mut self, by: Option<u32>) -> Option<Heard> {
        let sling = self.waiting.pop_front();
        self.working = sling.clone().map(|sling| Work { sling, by });
        sling
    }

    /// Puts back a sling that was taken and never reached its listener: it
    /// is the oldest again, and nobody works on it.
    pub fn put_back(&mut self, sling: Heard) {
        self.waiting.push_front(sling);
        self.working = None;
    }

    /// The listener said at `now` that it dealt with its sling (it set its
    /// status `done` or `blocked`): it counts as there for [`GRACE`], as
    /// after an ask, so its next ask still finds the slings its own.
    pub fn done(&mut self, now: Instant) {
        if self.working.take().is_some() {
            self.seen = Some(now);
        }
    }

    /// The process that works on the sling the listener took, when the
    /// listener named one. Once it is gone, so is the listener
    /// ([`Listener::gone`]).
    #[must_use]
    pub fn worker(&self) -> Option<u32> {
        self.working.as_ref().and_then(|work| work.by)
    }

    /// The `sling_sent` number of the sling the listener took and has not
    /// said it dealt with. A [`Listener::take`], [`Listener::done`] or
    /// [`Listener::release`] while there is one deals with it.
    #[must_use]
    pub fn working(&self) -> Option<u64> {
        self.working.as_ref().map(|work| work.sling.event)
    }

    /// The process that worked on the sling ended before it said it dealt
    /// with it: the sling waits again, the oldest, and nobody is there.
    pub fn gone(&mut self) {
        if let Some(Work { sling, .. }) = self.working.take() {
            self.waiting.push_front(sling);
        }
        self.seen = None;
    }

    /// Whether a listener is there at `now`: it asks, it asked or said it
    /// was done within [`GRACE`], or it works on a sling it took.
    #[must_use]
    pub fn attached(&self, now: Instant) -> bool {
        let lately = self.seen.is_some_and(|seen| now.saturating_duration_since(seen) < GRACE);
        self.asking > 0 || self.working.is_some() || lately
    }

    /// How many slings a listener has still to deal with: those that wait,
    /// and the one it works on.
    #[must_use]
    pub fn pending(&self) -> u32 {
        let working = u32::from(self.working.is_some());
        u32::try_from(self.waiting.len()).unwrap_or(u32::MAX).saturating_add(working)
    }

    /// Forgets who listened: the agent was stopped, or handed to a process.
    /// The slings that wait stay.
    pub fn release(&mut self) {
        self.seen = None;
        self.working = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sling(event: u64) -> Heard {
        Heard { event, canvas: "idea".into(), dir: "/p".into(), text: format!("sling {event}") }
    }

    #[test]
    fn slings_wait_for_a_listener_and_come_oldest_first() {
        let mut l = Listener::default();
        let now = Instant::now();
        assert!(!l.attached(now) && l.pending() == 0 && l.take(None).is_none());
        l.push(sling(1));
        l.push(sling(2));
        assert!(!l.attached(now), "slings waiting are not a listener");
        assert_eq!(l.pending(), 2);
        assert_eq!(l.take(None), Some(sling(1)));
        assert_eq!(l.pending(), 2, "one waits, one is worked on");
        assert_eq!(l.take(None), Some(sling(2)));
        assert_eq!(l.take(None), None);
        assert_eq!(l.pending(), 0);
    }

    #[test]
    fn a_listener_is_there_while_it_asks_works_or_asked_lately() {
        let mut l = Listener::default();
        let now = Instant::now();
        l.asked();
        assert!(l.attached(now + GRACE * 9), "an open ask");
        l.left(now);
        assert!(l.attached(now + GRACE / 2));
        assert!(!l.attached(now + GRACE), "it did not ask again");
        l.push(sling(1));
        l.take(None);
        assert!(l.attached(now + GRACE * 9), "it works on the sling it took");
        l.take(None);
        assert!(!l.attached(now + GRACE), "asking again ends the work");
    }

    #[test]
    fn a_listener_that_says_done_stays_for_a_grace_and_one_whose_process_is_gone_leaves() {
        let mut l = Listener::default();
        let now = Instant::now();
        l.push(sling(1));
        l.push(sling(2));
        l.take(Some(42));
        assert_eq!((l.worker(), l.working()), (Some(42), Some(1)));
        l.done(now + GRACE * 9);
        assert_eq!((l.worker(), l.pending()), (None, 1), "done ends the work");
        assert!(l.attached(now + GRACE * 9 + GRACE / 2), "it is about to ask again");
        assert!(!l.attached(now + GRACE * 10));

        l.take(Some(42));
        l.gone();
        assert!(!l.attached(now), "its process is gone");
        assert_eq!((l.worker(), l.working(), l.pending()), (None, None, 1));
        assert_eq!(l.take(None), Some(sling(2)), "the sling it worked on waits again");
        assert_eq!(l.worker(), None, "an ask that names no process");
        l.done(now);
        l.done(now + GRACE * 9);
        assert!(!l.attached(now + GRACE * 9), "done with nothing taken is not a sign of life");
    }

    #[test]
    fn a_sling_put_back_is_the_oldest_again_and_a_release_keeps_the_slings() {
        let mut l = Listener::default();
        let now = Instant::now();
        l.push(sling(1));
        l.push(sling(2));
        let taken = l.take(None).unwrap();
        l.put_back(taken);
        assert!(!l.attached(now), "nobody works on it");
        assert_eq!(l.pending(), 2);
        assert_eq!(l.take(None), Some(sling(1)));
        l.left(now);
        l.release();
        assert!(!l.attached(now));
        assert_eq!(l.take(None), Some(sling(2)));
    }
}
