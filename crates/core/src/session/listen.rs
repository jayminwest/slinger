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
    /// When an ask last ended.
    seen: Option<Instant>,
    /// It took a sling and has not asked for the next.
    working: bool,
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

    /// The oldest sling that waits, taken by the listener that asks. Asking
    /// says its last sling is dealt with, so it works only if it gets one.
    pub fn take(&mut self) -> Option<Heard> {
        let sling = self.waiting.pop_front();
        self.working = sling.is_some();
        sling
    }

    /// Puts back a sling that was taken and never reached its listener: it
    /// is the oldest again, and nobody works on it.
    pub fn put_back(&mut self, sling: Heard) {
        self.waiting.push_front(sling);
        self.working = false;
    }

    /// Whether a listener is there at `now`: it asks, it asked within
    /// [`GRACE`], or it works on a sling it took.
    #[must_use]
    pub fn attached(&self, now: Instant) -> bool {
        let lately = self.seen.is_some_and(|seen| now.saturating_duration_since(seen) < GRACE);
        self.asking > 0 || self.working || lately
    }

    /// How many slings a listener has still to deal with: those that wait,
    /// and the one it works on.
    #[must_use]
    pub fn pending(&self) -> u32 {
        u32::try_from(self.waiting.len()).unwrap_or(u32::MAX).saturating_add(self.working.into())
    }

    /// Forgets who listened: the agent was stopped, or handed to a process.
    /// The slings that wait stay.
    pub fn release(&mut self) {
        self.seen = None;
        self.working = false;
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
        assert!(!l.attached(now) && l.pending() == 0 && l.take().is_none());
        l.push(sling(1));
        l.push(sling(2));
        assert!(!l.attached(now), "slings waiting are not a listener");
        assert_eq!(l.pending(), 2);
        assert_eq!(l.take(), Some(sling(1)));
        assert_eq!(l.pending(), 2, "one waits, one is worked on");
        assert_eq!(l.take(), Some(sling(2)));
        assert_eq!(l.take(), None);
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
        l.take();
        assert!(l.attached(now + GRACE * 9), "it works on the sling it took");
        l.take();
        assert!(!l.attached(now + GRACE), "asking again ends the work");
    }

    #[test]
    fn a_sling_put_back_is_the_oldest_again_and_a_release_keeps_the_slings() {
        let mut l = Listener::default();
        let now = Instant::now();
        l.push(sling(1));
        l.push(sling(2));
        let taken = l.take().unwrap();
        l.put_back(taken);
        assert!(!l.attached(now), "nobody works on it");
        assert_eq!(l.pending(), 2);
        assert_eq!(l.take(), Some(sling(1)));
        l.left(now);
        l.release();
        assert!(!l.attached(now));
        assert_eq!(l.take(), Some(sling(2)));
    }
}
