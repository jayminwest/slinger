//! Speaking the sling's prompt (SPEC D2, section 11), or into any open
//! entry (a note of feedback, a page's new name): Flick dictates, and
//! types what it heard into the focused field, the entry. Pure:
//! `canvases` hands in the `dictate` command, the entry closing and Flick's
//! answers, and reads back the request to send and the lines to show.
//!
//! The `dictate` command starts Flick's dictation; the next one stops it,
//! and Flick then writes the text into the entry (Flick's own pill shows it
//! listening and transcribing). Closing the entry cancels a dictation still
//! going. A refusal, or a Flick that is not running, shows once under the
//! entry, with what to do; the entry stays and typing works as before.

use crate::backend::Problem;
use crate::config::{Rgb, Theme};

/// What the app asks Flick's dictation to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verb {
    Start,
    Stop,
    Cancel,
}

impl Verb {
    /// The verb's word in Flick's protocol (`["dictation","start"]`).
    pub fn word(self) -> &'static str {
        match self {
            Verb::Start => "start",
            Verb::Stop => "stop",
            Verb::Cancel => "cancel",
        }
    }
}

/// Where a dictation is.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum Stage {
    #[default]
    Idle,
    /// Asked to start; Flick has not answered.
    Starting,
    /// Flick is recording.
    Listening,
    /// Asked to stop; Flick has not answered.
    Stopping,
}

/// The dictation into the sling's entry, and why the last one failed.
#[derive(Debug, Default)]
pub struct Dictation {
    stage: Stage,
    problem: Option<Problem>,
}

impl Dictation {
    /// The `dictate` command: start when idle, stop while Flick listens.
    /// Nothing while an answer from Flick is still due.
    pub fn toggle(&mut self) -> Option<Verb> {
        let (stage, verb) = match self.stage {
            Stage::Idle => (Stage::Starting, Verb::Start),
            Stage::Listening => (Stage::Stopping, Verb::Stop),
            Stage::Starting | Stage::Stopping => return None,
        };
        (self.stage, self.problem) = (stage, None);
        Some(verb)
    }

    /// Whether Flick is asked to listen or listens: the pen's dictate
    /// button stops it ([`crate::scene::sling::pen`]).
    pub fn listening(&self) -> bool {
        matches!(self.stage, Stage::Starting | Stage::Listening)
    }

    /// The entry closed: a dictation still going is cancelled, and the
    /// last failure goes with the entry.
    pub fn closed(&mut self) -> Option<Verb> {
        let going = self.stage != Stage::Idle;
        (self.stage, self.problem) = (Stage::Idle, None);
        going.then_some(Verb::Cancel)
    }

    /// Flick's answer to `verb`. An answer to a request since overtaken
    /// (one sent before the entry closed) changes nothing. Returns whether
    /// anything shown changed.
    pub fn answered(&mut self, verb: Verb, answer: Result<(), Problem>) -> bool {
        let next = match (verb, self.stage) {
            (Verb::Start, Stage::Starting) => Stage::Listening,
            (Verb::Stop, Stage::Stopping) => Stage::Idle,
            _ => return false,
        };
        match answer {
            Ok(()) => self.stage = next,
            Err(problem) => (self.stage, self.problem) = (Stage::Idle, Some(problem)),
        }
        true
    }

    /// The lines shown under the entry, in `theme`'s colours: that Flick
    /// listens, or why the last dictation failed and what to do.
    pub fn lines(&self, theme: &Theme) -> Vec<(String, Rgb)> {
        match (self.stage, &self.problem) {
            (Stage::Starting | Stage::Listening, _) => {
                vec![("● Flick is listening: dictate again when done".to_string(), theme.muted)]
            }
            (Stage::Stopping, _) => vec![("Flick is writing it down…".to_string(), theme.muted)],
            (Stage::Idle, None) => Vec::new(),
            (Stage::Idle, Some(problem)) => {
                let mut lines = vec![(format!("✗ {}", problem.message), theme.error)];
                lines.extend(problem.hint.clone().map(|hint| (hint, theme.muted)));
                lines
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(dictation: &Dictation) -> Vec<String> {
        dictation.lines(&Theme::default()).into_iter().map(|(text, _)| text).collect()
    }

    fn not_running() -> Problem {
        Problem { message: "Flick is not running".into(), hint: Some("start Flick".into()) }
    }

    #[test]
    fn dictate_starts_then_stops_and_waits_for_each_answer() {
        let mut dictation = Dictation::default();
        assert_eq!(texts(&dictation), Vec::<String>::new());
        assert_eq!(dictation.toggle(), Some(Verb::Start));
        assert!(dictation.listening());
        assert_eq!(dictation.toggle(), None, "the start's answer is due");
        assert_eq!(texts(&dictation), ["● Flick is listening: dictate again when done"]);
        assert!(!dictation.answered(Verb::Stop, Ok(())), "not asked to stop");
        assert!(dictation.answered(Verb::Start, Ok(())));
        assert_eq!(dictation.toggle(), Some(Verb::Stop));
        assert_eq!(texts(&dictation), ["Flick is writing it down…"]);
        assert!(!dictation.listening());
        assert!(dictation.answered(Verb::Stop, Ok(())));
        assert_eq!(texts(&dictation), Vec::<String>::new());
        assert_eq!(dictation.closed(), None, "nothing going: nothing to cancel");
        assert_eq!(dictation.toggle(), Some(Verb::Start), "and again");
    }

    #[test]
    fn a_failure_shows_once_with_its_hint_and_dictate_tries_again() {
        let mut dictation = Dictation::default();
        dictation.toggle();
        assert!(dictation.answered(Verb::Start, Err(not_running())));
        assert_eq!(texts(&dictation), ["✗ Flick is not running", "start Flick"]);
        let colours: Vec<Rgb> =
            dictation.lines(&Theme::default()).into_iter().map(|l| l.1).collect();
        assert_eq!(colours, [Theme::default().error, Theme::default().muted]);
        assert_eq!(dictation.toggle(), Some(Verb::Start), "the failure goes; it tries again");
        assert_eq!(texts(&dictation).len(), 1);
        dictation.answered(Verb::Start, Ok(()));
        dictation.toggle();
        let refused = Problem { message: "dictation: not recording".into(), hint: None };
        assert!(dictation.answered(Verb::Stop, Err(refused)));
        assert_eq!(texts(&dictation), ["✗ dictation: not recording"]);
        assert_eq!(dictation.closed(), None);
        assert_eq!(texts(&dictation), Vec::<String>::new(), "it goes with the entry");
    }

    #[test]
    fn closing_the_entry_cancels_a_dictation_going_and_its_late_answer_is_ignored() {
        for answered in [false, true] {
            let mut dictation = Dictation::default();
            dictation.toggle();
            if answered {
                dictation.answered(Verb::Start, Ok(()));
                dictation.toggle();
            }
            assert_eq!(dictation.closed(), Some(Verb::Cancel));
            assert!(!dictation.answered(Verb::Start, Ok(())));
            assert!(!dictation.answered(Verb::Stop, Ok(())));
            assert!(!dictation.answered(Verb::Cancel, Ok(())));
            assert_eq!(texts(&dictation), Vec::<String>::new());
        }
        assert_eq!(Verb::Cancel.word(), "cancel");
    }
}
