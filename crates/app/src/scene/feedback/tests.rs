use super::*;
use crate::config::Rgb;
use crate::scene::sling::pen::beside;

const T: Theme = Theme {
    background: Rgb { r: 0, g: 0, b: 0 },
    text: Rgb { r: 1, g: 1, b: 1 },
    muted: Rgb { r: 2, g: 2, b: 2 },
    error: Rgb { r: 3, g: 3, b: 3 },
    focus: Rgb { r: 4, g: 4, b: 4 },
    done: Rgb { r: 5, g: 5, b: 5 },
};

const PANE: Rect = Rect { x: 100.0, y: 0.0, width: 600.0, height: 400.0 };
const RETURN: Chord = Chord::new(0, '\r');
const ESCAPE: Chord = Chord::new(0, '\u{1b}');

fn context() -> Context {
    Context {
        workspace: Some("notes".into()),
        canvas: Some("notes".into()),
        page: Some("Page 1".into()),
        tool: Some("lasso".into()),
        selected: 2,
    }
}

/// An open entry with `text` typed into it.
fn typed(text: &str) -> Feedback {
    let mut feedback = Feedback::default();
    assert!(feedback.open(PANE, context()));
    feedback.typed(text);
    feedback
}

fn texts(feedback: &Feedback) -> Vec<(String, Rgb)> {
    feedback.labels(&T).into_iter().map(|l| (l.text, l.colour)).collect()
}

#[test]
fn the_entry_opens_in_the_pane_labelled_feedback_with_its_keys() {
    let feedback = typed("the lasso is slow");
    let entry = feedback.entry(&T).unwrap();
    assert_eq!(entry.title.label.text, "Feedback");
    assert_eq!(entry.keys.label.text, "↩ saves · esc cancels");
    assert_eq!((entry.text.as_str(), entry.dictate), ("the lasso is slow", None));
    let inside = |r: Rect| r.x >= PANE.x && r.x + r.width <= PANE.x + PANE.width;
    assert!(inside(entry.frame) && entry.frame.y > 40.0, "{:?}", entry.frame);
    assert_eq!((entry.frame, entry.done), (beside(PANE), Some("Save")), "the pen's buttons");
    let on = Point { x: entry.frame.x + 1.0, y: entry.frame.y + 1.0 };
    assert!(feedback.covers(on) && !feedback.covers(Point { x: 0.0, y: 0.0 }));
    assert_eq!(texts(&feedback), []);
}

#[test]
fn return_saves_the_note_with_its_context_and_a_save_flashes_once() {
    let mut feedback = typed("the lasso is slow");
    assert_eq!(feedback.key(RETURN), Some(Key::Send));
    assert_eq!(feedback.key(Chord::new(0, 'w')), Some(Key::Type));
    let note = feedback.send().unwrap();
    assert_eq!(note, Note { text: "the lasso is slow".into(), context: context() });
    assert!(!feedback.is_typing() && feedback.entry(&T).is_none());
    assert_eq!(texts(&feedback), [("saving feedback…".to_string(), T.muted)]);
    assert!(!feedback.open(PANE, Context::default()), "one at a time");
    assert!(!feedback.covers(Point { x: 400.0, y: 130.0 }));

    assert!(feedback.answered(Ok(())));
    assert_eq!((feedback.entry(&T), texts(&feedback)), (None, vec![]));
    assert_eq!(feedback.flash().as_deref(), Some("Feedback saved"));
    assert_eq!(feedback.flash(), None, "once");
    assert!(!feedback.answered(Ok(())), "nothing was saving");
}

#[test]
fn a_failure_opens_the_entry_again_with_its_text_and_the_problem() {
    let mut feedback = typed("slow");
    feedback.send().unwrap();
    let problem =
        Problem { message: "slingerd does not answer".into(), hint: Some("start it".into()) };
    assert!(feedback.answered(Err(problem)));
    assert_eq!(feedback.entry(&T).map(|e| e.text), Some("slow".into()));
    let lines = texts(&feedback);
    assert_eq!(
        lines,
        [("✗ slingerd does not answer".into(), T.error), ("start it".into(), T.muted)]
    );
    assert_eq!(feedback.flash(), None);
    assert!(feedback.open(PANE, Context::default()), "open again keeps the text");
    assert_eq!(feedback.entry(&T).map(|e| e.text), Some("slow".into()));
}

#[test]
fn escape_cancels_and_an_empty_note_saves_nothing() {
    let mut feedback = typed("never mind");
    assert_eq!(feedback.key(ESCAPE), Some(Key::Cancel));
    assert!(feedback.cancel());
    assert!(!feedback.cancel() && feedback.key(ESCAPE).is_none(), "closed");
    feedback.typed("ignored while closed");
    assert_eq!(feedback.send(), None);

    let mut blank = typed("  \n ");
    assert_eq!(blank.send(), None);
    assert!(!blank.is_typing() && blank.entry(&T).is_none(), "it closed");
    assert_eq!(blank.flash(), None);
}

#[test]
fn a_narrow_pane_keeps_the_entry_inside_it() {
    let mut feedback = Feedback::default();
    let narrow = Rect { x: 0.0, y: 0.0, width: 200.0, height: 100.0 };
    feedback.open(narrow, Context::default());
    let frame = feedback.entry(&T).unwrap().frame;
    assert!(frame.x >= 0.0 && frame.x + frame.width <= 200.0, "{frame:?}");
}

fn recent(text: &str) -> Recent {
    Recent { text: text.into(), when: "11-11 14:02".into() }
}

#[test]
fn the_open_entry_lists_the_recent_notes_under_any_problem() {
    let mut feedback = Feedback::default();
    assert!(!feedback.listed(vec![recent("old")]), "closed: nothing shown changes");
    assert!(feedback.open(PANE, context()));
    let header = ("Recent feedback".to_string(), T.muted);
    assert_eq!(texts(&feedback), [header.clone(), ("11-11 14:02 · old".into(), T.muted)]);
    assert!(!feedback.listed(vec![recent("old")]), "the same list");

    let many: Vec<_> = (0..9).map(|n| recent(&n.to_string())).collect();
    assert!(feedback.listed(many));
    let lines = feedback.labels(&T);
    assert_eq!(lines.len(), 1 + RECENT);
    assert!(lines.windows(2).all(|two| two[0].frame.y < two[1].frame.y), "one under another");
    assert!(lines[0].frame.y > feedback.entry(&T).unwrap().frame.y, "under the entry");

    feedback.typed("slow");
    feedback.send().unwrap();
    assert_eq!(texts(&feedback), [("saving feedback…".to_string(), T.muted)]);
    feedback.answered(Err(Problem { message: "gone".into(), hint: None }));
    let lines = texts(&feedback);
    assert_eq!(lines[..2], [("✗ gone".to_string(), T.error), header]);

    // What dictation says goes under them all.
    let said = feedback.under(&T, vec![("● Flick is listening".into(), T.muted)]);
    let last = feedback.labels(&T).last().map(|l| l.frame.y).unwrap();
    assert!(said.len() == 1 && said[0].frame.y > last, "{said:?}");
}
