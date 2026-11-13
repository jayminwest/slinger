use super::*;
use crate::config::Rgb;
use crate::scene::sling::pen::beside;
use crate::scene::taps::Tap;

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
    assert_eq!(entry.text, "the lasso is slow");
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
    let when = "11-11 14:02".into();
    Recent { ts: format!("ts {text}"), text: text.into(), when, resolved: None }
}

#[test]
fn the_open_entry_lists_the_recent_notes_under_any_problem() {
    let mut feedback = Feedback::default();
    assert!(!feedback.listed(vec![recent("old")]), "closed: nothing shown changes");
    assert!(feedback.open(PANE, context()));
    let header = ("Recent feedback".to_string(), T.muted);
    assert_eq!(texts(&feedback), [header.clone(), ("11-11 14:02 · old".into(), T.muted)]);
    assert!(!feedback.listed(vec![recent("old")]), "the same list");

    let many: Vec<_> = (0..30).map(|n| recent(&n.to_string())).collect();
    assert!(feedback.listed(many));
    let lines = feedback.labels(&T);
    assert_eq!(lines.len(), 1 + feedback.ticks(&T).1.len(), "a header, then the notes");
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

/// The open entry in a pane `height` tall, listing `n` notes.
fn listing(height: f64, n: usize) -> Feedback {
    let mut feedback = Feedback::default();
    assert!(feedback.open(Rect { height, ..PANE }, context()));
    feedback.listed((0..n).map(|n| recent(&n.to_string())).collect());
    feedback
}

#[test]
fn a_tall_pane_lists_as_many_more_notes_as_fit_with_room_to_spare() {
    let tall = listing(1200.0, 30);
    let lines = tall.labels(&T);
    assert!(lines.len() > 1 + RECENT && lines.len() <= 1 + LISTED, "{}", lines.len());
    let said = tall.under(&T, vec![("a".into(), T.muted), ("b".into(), T.muted)]);
    assert!(said[1].frame.y + said[1].frame.height <= 1200.0, "dictation still fits");
    assert_eq!(listing(1200.0, 3).labels(&T).len(), 1 + 3, "only what there is");
    assert_eq!(listing(300.0, 30).labels(&T).len(), 1 + RECENT, "a short pane: five still");
}

#[test]
fn a_tick_stands_at_each_notes_end_and_resolves_it() {
    let mut feedback = listing(400.0, 3);
    let (labels, targets) = feedback.ticks(&T);
    let rows = feedback.labels(&T).split_off(1);
    assert_eq!(labels.iter().map(|l| l.text.as_str()).collect::<Vec<_>>(), ["✓"; 3]);
    let frames: Vec<_> = labels.iter().map(|l| l.frame).collect();
    let taps: Vec<_> = targets.iter().map(|t| t.1).collect();
    assert_eq!(targets.iter().map(|t| t.0).collect::<Vec<_>>(), frames);
    assert_eq!(taps, (0..3).map(Tap::Resolve).collect::<Vec<_>>());
    let level = |(tick, row): (&Rect, &Label)| (tick.y - row.frame.y).abs() < 0.01;
    assert!(frames.iter().zip(&rows).all(level), "level with its note");
    let ends = |(tick, row): (&Rect, &Label)| tick.x >= row.frame.x + row.frame.width;
    assert!(frames.iter().zip(&rows).all(ends), "at its end");
    assert_eq!(feedback.resolve(3), None, "no such note");
    assert_eq!(feedback.resolve(1).as_deref(), Some("ts 1"));
    assert_eq!(feedback.ticks(&T).1.len(), 2, "it leaves the list");

    assert!(feedback.cancel());
    assert_eq!((feedback.resolve(0), feedback.ticks(&T).1.len()), (None, 0), "closed");
    assert!(!feedback.unresolved(Problem { message: "late".into(), hint: None }));
}

#[test]
fn a_note_that_does_not_resolve_says_why_under_the_entry_until_the_next_try() {
    let mut feedback = listing(400.0, 3);
    let problem = Problem { message: "no note".into(), hint: Some("list them".into()) };
    assert!(feedback.unresolved(problem));
    let lines = texts(&feedback);
    assert_eq!(lines[..2], [("✗ no note".into(), T.error), ("list them".into(), T.muted)]);
    let first = feedback.ticks(&T).1[0].0.y - feedback.labels(&T)[3].frame.y;
    assert!(first.abs() < 0.01, "the notes under the problem");
    assert_eq!(feedback.resolve(0).as_deref(), Some("ts 0"));
    assert_eq!(texts(&feedback)[0].0, "Recent feedback", "the problem goes");
}

/// slinger-c57a12: listing all, the resolved notes show how they were
/// resolved, with no "✓"; the open ones keep theirs, by their own index.
#[test]
fn all_feedback_lists_the_resolved_notes_apart_with_no_tick() {
    let mut feedback = Feedback::default();
    feedback.listed(vec![recent("old")]);
    feedback.list_all(true);
    assert!(feedback.lists_all());
    assert!(feedback.open(Rect { height: 400.0, ..PANE }, context()));
    assert_eq!(texts(&feedback), [], "the open notes listed before go");
    let done =
        Recent { resolved: Some("resolved 11-12 09:30: slinger-1234".into()), ..recent("b") };
    assert!(feedback.listed(vec![recent("a"), done, recent("c")]));
    let lines: Vec<_> = texts(&feedback).into_iter().map(|(text, _)| text).collect();
    let all =
        ["All feedback", "11-11 14:02 · a", "11-11 14:02 · b · resolved 11-12 09:30: slinger-1234"];
    assert_eq!(lines[..3], all);
    let (labels, targets) = feedback.ticks(&T);
    assert_eq!(labels.len(), 2, "none for the resolved note");
    assert_eq!(targets.iter().map(|t| t.1).collect::<Vec<_>>(), [Tap::Resolve(0), Tap::Resolve(2)]);
    let rows = feedback.labels(&T);
    assert!((targets[1].0.y - rows[3].frame.y).abs() < 0.01, "level with its own note");
    assert_eq!(feedback.resolve(2).as_deref(), Some("ts c"));

    feedback.list_all(false);
    assert!(!feedback.lists_all());
    assert_eq!(texts(&feedback), [], "until the open notes are listed");
}

/// slinger-d03fb8: the open entry stands on a card in the panel's colour,
/// not the canvas's: from its label to its last line, with room around.
#[test]
fn the_entry_and_its_lines_stand_on_a_card_in_the_panel_s_colour() {
    let mut feedback = typed("");
    let panel = Some(style::panel(&T));
    let real = Theme::default();
    assert_ne!(style::panel(&real), real.background, "not the canvas's colour");
    let entry = feedback.entry(&T).unwrap();
    assert_eq!((entry.title.label.fill, entry.keys.label.fill), (panel, panel));
    let box_ = entry.frame;
    let card = feedback.card(&T).unwrap();
    assert_eq!((card.text.as_str(), card.fill), ("", panel));
    let (x, y, width) = (box_.x - CARD, box_.y - SMALL - CARD, box_.width + 2.0 * CARD);
    assert_eq!((card.frame.x, card.frame.y, card.frame.width), (x, y, width), "with room");
    let tall = SMALL + HEIGHT + SMALL + 2.0 * CARD;
    assert!((card.frame.height - tall).abs() < 1e-9, "the label, the box and the keys");

    feedback.listed(vec![recent("old")]);
    let lines = feedback.labels(&T);
    assert!(lines.iter().all(|l| l.fill == panel), "{lines:?}");
    let last = lines.last().unwrap().frame;
    let card = feedback.card(&T).unwrap().frame;
    let end = last.y + last.height + CARD;
    assert!((card.y + card.height - end).abs() < 1e-9, "down to the last note");
    let said = feedback.under(&T, vec![("a".into(), T.muted)]);
    assert_eq!(said[0].fill, panel, "what dictation says too");

    feedback.typed("slow");
    feedback.send().unwrap();
    assert_eq!(feedback.card(&T), None, "saving: only its line");
    assert_eq!(feedback.labels(&T)[0].fill, panel);
}
