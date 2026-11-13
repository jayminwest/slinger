//! How many of the messages written to a process it has not answered: the
//! session's `pending` (docs/service.md, "The process").
//!
//! A `result` line ends a turn and answers a message. But Claude Code takes a
//! message that arrives mid-turn into that turn, so two messages can get one
//! result (slinger-b84853), and a count of results against sends would stay
//! above 0 for good. A message it has not taken in starts a turn of its own
//! right after the result. So once a result came and the process said nothing
//! more for [`QUIET`], every message it was sent is answered.

use std::time::{Duration, Instant};

/// How long after a result a message still unanswered may wait to start its
/// own turn; after that it was taken into the turn that ended.
pub const QUIET: Duration = Duration::from_secs(2);

/// The turns of one process.
#[derive(Debug, Default)]
pub struct Turns {
    /// Messages written that no result has answered.
    sent: u32,
    /// A turn runs: a message was sent, or the process spoke, since the
    /// last result.
    running: bool,
    /// When the last result came.
    answered: Option<Instant>,
}

impl Turns {
    /// The messages sent that are not answered, as of `now`: at least 1
    /// while a turn runs.
    pub fn pending(&mut self, now: Instant) -> u32 {
        self.settle(now);
        if self.running { self.sent.max(1) } else { self.sent }
    }

    /// A message is about to be written. Counted first: its answer can come
    /// back before the write returns (slinger-d71082).
    pub fn send(&mut self, now: Instant) {
        self.settle(now);
        self.sent += 1;
        self.running = true;
    }

    /// The message [`Turns::send`] counted did not get written.
    pub fn unsend(&mut self) {
        self.sent = self.sent.saturating_sub(1);
        self.running &= self.sent > 0;
    }

    /// The process wrote a line: a result (`answer`) or anything else.
    pub fn heard(&mut self, answer: bool, now: Instant) {
        self.settle(now);
        if answer {
            self.sent = self.sent.saturating_sub(1);
            self.running = false;
            self.answered = Some(now);
        } else if self.sent > 0 {
            // A message waiting after the last result has started its turn.
            self.running = true;
        }
    }

    /// Takes the messages left after a result, with nothing heard since for
    /// [`QUIET`], as answered by it.
    fn settle(&mut self, now: Instant) {
        let quiet = self.answered.is_some_and(|at| now.duration_since(at) >= QUIET);
        if !self.running && quiet {
            self.sent = 0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_message_taken_into_the_running_turn_is_answered_by_its_result() {
        let (mut turns, t) = (Turns::default(), Instant::now());
        turns.send(t);
        turns.heard(false, t);
        turns.send(t);
        assert_eq!(turns.pending(t), 2);
        turns.heard(true, t);
        assert_eq!(turns.pending(t), 1, "it may yet start a turn of its own");
        assert_eq!(turns.pending(t + QUIET), 0, "it did not: the result answered both");
        turns.send(t + QUIET);
        turns.heard(true, t + QUIET);
        assert_eq!(turns.pending(t + QUIET), 0, "a later send is not counted against");
    }

    #[test]
    fn messages_answered_in_turns_of_their_own_stay_pending_between_them() {
        let (mut turns, t) = (Turns::default(), Instant::now());
        turns.send(t);
        turns.send(t);
        turns.heard(true, t);
        let later = t + QUIET + QUIET;
        turns.heard(false, t + QUIET / 2);
        assert_eq!(turns.pending(later), 1, "its turn runs, however long");
        turns.heard(true, later);
        assert_eq!(turns.pending(later), 0);
    }

    #[test]
    fn output_with_nothing_sent_is_no_turn_and_an_unwritten_message_is_none() {
        let (mut turns, t) = (Turns::default(), Instant::now());
        turns.heard(false, t);
        assert_eq!(turns.pending(t), 0);
        turns.send(t);
        assert_eq!(turns.pending(t), 1);
        turns.unsend();
        assert_eq!(turns.pending(t), 0);
    }
}
