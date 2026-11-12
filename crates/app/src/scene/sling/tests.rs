use super::*;
use crate::config::Rgb;

const T: Theme = Theme {
    background: Rgb { r: 0, g: 0, b: 0 },
    text: Rgb { r: 1, g: 1, b: 1 },
    muted: Rgb { r: 2, g: 2, b: 2 },
    error: Rgb { r: 3, g: 3, b: 3 },
    focus: Rgb { r: 4, g: 4, b: 4 },
    done: Rgb { r: 5, g: 5, b: 5 },
};

const PANE: Rect = Rect { x: 100.0, y: 0.0, width: 900.0, height: 400.0 };
const BOX: Rect = Rect { x: 200.0, y: 100.0, width: 50.0, height: 40.0 };

fn selection() -> Selection {
    Selection { canvas: "notes".into(), page: "p1".into(), ids: vec![ItemId(1), ItemId(4)] }
}

/// The open entry, to scout, with no dictate key.
fn entry(sling: &Sling) -> Option<Entry> {
    sling.entry(&T, Some("scout"), None)
}

fn texts(sling: &Sling) -> Vec<(String, Rgb)> {
    sling.labels(&T).into_iter().map(|l| (l.text, l.colour)).collect()
}

fn no_session() -> Problem {
    Problem {
        message: "scout is not running".into(),
        hint: Some("start it with 'slinger session start notes'".into()),
    }
}

/// An open entry with `text` typed into it.
fn typed(text: &str) -> Sling {
    let mut sling = Sling::default();
    assert!(sling.open(&selection(), BOX, PANE));
    sling.typed(text);
    sling
}

#[test]
fn the_command_opens_an_empty_entry_under_the_selection() {
    let sling = typed("");
    let entry = entry(&sling).unwrap();
    assert_eq!(entry.text, "");
    assert_eq!(entry.placeholder, "what to ask about 2 items (or nothing)");
    assert_eq!(entry.frame, Rect { x: 200.0, y: 164.0, width: WIDTH, height: HEIGHT });
    assert!(sling.is_typing());
    assert!(sling.covers(Point { x: 210.0, y: 170.0 }));
    assert!(!sling.covers(Point { x: 210.0, y: 120.0 }), "the selection is not the entry");
    assert!(!sling.covers(Point { x: 210.0, y: 150.0 }), "the label is not the box");
    assert_eq!(texts(&sling), []);
}

#[test]
fn the_entry_is_a_box_with_a_focus_coloured_border_and_the_text_inside() {
    let e = entry(&typed("")).unwrap();
    let frame = e.frame;
    assert_eq!(e.frame.height, 44.0);
    assert_eq!(e.border, Border { frame, width: EDGE, colour: T.focus });
    assert_eq!((e.fill, e.colour), (T.background, T.text));
    assert!(e.field.y > frame.y && e.field.y + e.field.height < frame.y + frame.height);
    assert!(e.field.x > frame.x && e.field.x + e.field.width < frame.x + frame.width);
}

#[test]
fn the_entry_is_labelled_with_its_agent_and_its_keys_are_beneath() {
    let sling = typed("");
    let e = sling.entry(&T, Some("scout"), Some(Chord::new(crate::commands::ALT, 'd'))).unwrap();
    let frame = e.frame;
    let (title, keys) = (&e.title.label, &e.keys.label);
    assert_eq!((title.text.as_str(), title.colour), ("Sling to scout", T.muted));
    assert_eq!(title.frame.y + title.frame.height, frame.y, "right above the box");
    assert_eq!(e.title.font.size, Some(11.0));
    assert_eq!((keys.text.as_str(), keys.colour), ("↩ sends · esc cancels", T.muted));
    assert_eq!(keys.frame.y, frame.y + frame.height, "right under the box");
    let dictate = e.dictate.unwrap();
    assert_eq!(dictate.label.text, "⌥D dictate");
    assert_eq!((dictate.label.frame, dictate.font), (keys.frame, e.keys.font), "on the keys' line");
    assert_eq!(dictate.label.fill, None, "it does not cover the keys");

    assert_eq!(e.done, Some("Sling"), "the pen's buttons: ✓ Sling, Dictate, ✕");

    let alone = sling.entry(&T, None, None).unwrap();
    assert_eq!(alone.title.label.text, "Sling", "a workspace with no agent");
    assert_eq!(alone.dictate, None, "no key dictates: nothing to show");
}

#[test]
fn the_entry_stays_inside_its_pane() {
    let low = Rect { y: 360.0, ..BOX };
    assert_eq!(place(low, PANE).y, 360.0 - GAP - SMALL - HEIGHT, "above when no room below");
    let right = Rect { x: 980.0, ..BOX };
    assert_eq!(place(right, PANE).x, 1000.0 - WIDTH - pen::ROOM - GAP, "room for the buttons");
    let left = Rect { x: 0.0, ..BOX };
    assert_eq!(place(left, PANE).x, 100.0 + GAP);
    let narrow = Rect { width: 420.0, ..PANE };
    let squeezed = place(BOX, narrow);
    assert!(squeezed.width > 0.0 && squeezed.width < WIDTH, "narrower: {squeezed:?}");
    let end = squeezed.x + squeezed.width + pen::ROOM;
    assert!((end - (narrow.x + narrow.width - GAP)).abs() < 0.01, "buttons inside: {squeezed:?}");
    let tall = Rect { y: 10.0, height: 390.0, ..BOX };
    assert_eq!(place(tall, PANE).y, GAP + SMALL, "its label never above the pane");
    let room = Rect { y: PANE.height - GAP - SMALL - HEIGHT - SMALL - GAP - BOX.height, ..BOX };
    assert_eq!(place(room, PANE).y, room.y + BOX.height + GAP + SMALL, "its keys just fit below");
}

#[test]
fn return_and_escape_are_the_entry_s_every_other_key_types() {
    let mut sling = Sling::default();
    assert_eq!(sling.key(Chord::new(0, '\r')), None, "closed: the keymap's");
    sling.open(&selection(), BOX, PANE);
    assert_eq!(sling.key(Chord::new(0, '\r')), Some(Key::Send));
    assert_eq!(sling.key(Chord::new(0, '\u{1b}')), Some(Key::Cancel));
    assert_eq!(sling.key(Chord::new(0, '\u{7f}')), Some(Key::Type), "delete edits the text");
    assert_eq!(sling.key(Chord::new(crate::commands::SHIFT, 'q')), Some(Key::Type));
}

#[test]
fn typing_sets_the_prompt_and_send_waits_for_the_ink_then_goes() {
    let mut sling = typed("what is missing?");
    assert_eq!(entry(&sling).unwrap().text, "what is missing?");
    assert!(sling.send());
    assert!(!sling.is_typing() && entry(&sling).is_none());
    sling.typed("ignored once sent");
    assert_eq!(texts(&sling), [("slinging 2 items: what is missing?".into(), T.muted)]);
    assert_eq!(sling.canvas(), Some("notes"), "where its agent is found");
    assert_eq!(sling.ready(|_| false), None, "ink of the canvas still on its way");
    let ask = sling.ready(|canvas| canvas == "notes").unwrap();
    assert_eq!(
        ask,
        Ask {
            canvas: "notes".into(),
            page: "p1".into(),
            ids: vec![ItemId(1), ItemId(4)],
            prompt: "what is missing?".into(),
        }
    );
    assert_eq!(sling.ready(|_| true), None, "it goes once");
    assert!(!sling.open(&selection(), BOX, PANE), "one sling at a time");
    assert_eq!(sling.answered(Ok(())), Answered::Closed);
    assert_eq!((entry(&sling), texts(&sling)), (None, vec![]), "it went: nothing more shows");
    assert_eq!(sling.canvas(), None);
    assert_eq!(sling.answered(Ok(())), Answered::Not, "no sling on its way");
}

#[test]
fn an_empty_prompt_is_a_sling_too() {
    let mut sling = typed("  ");
    sling.typed("");
    assert!(sling.send());
    assert_eq!(texts(&sling), [("slinging 2 items…".into(), T.muted)]);
    assert_eq!(sling.ready(|_| true).unwrap().prompt, "");
}

#[test]
fn escape_cancels_and_nothing_goes() {
    let mut sling = typed("never mind");
    assert!(sling.cancel());
    assert!(!sling.is_typing());
    assert_eq!((entry(&sling), texts(&sling)), (None, vec![]));
    assert_eq!(sling.ready(|_| true), None);
    assert!(!sling.cancel() && !sling.send(), "closed: nothing to cancel or send");
    assert!(sling.open(&selection(), BOX, PANE));
    assert_eq!(entry(&sling).unwrap().text, "", "a new sling starts empty");
}

#[test]
fn opening_again_moves_the_entry_and_keeps_the_text() {
    let mut sling = typed("half a thought");
    let one = Selection { ids: vec![ItemId(9)], ..selection() };
    assert!(sling.open(&one, Rect { x: 300.0, ..BOX }, PANE));
    let entry = entry(&sling).unwrap();
    assert_eq!((entry.text.as_str(), entry.frame.x), ("half a thought", 300.0));
    assert_eq!(entry.placeholder, "what to ask about 1 item (or nothing)");
}

#[test]
fn a_failure_shows_its_message_and_hint_under_the_entry_and_return_tries_again() {
    let mut sling = typed("why?");
    sling.send();
    sling.ready(|_| true).unwrap();
    assert_eq!(sling.answered(Err(no_session())), Answered::Reopened);
    assert!(sling.is_typing());
    let entry = entry(&sling).unwrap();
    assert_eq!(entry.text, "why?", "the prompt is kept");
    assert_eq!(
        texts(&sling),
        [
            ("✗ scout is not running".into(), T.error),
            ("start it with 'slinger session start notes'".into(), T.muted),
        ]
    );
    let lines = sling.labels(&T);
    let keys = entry.keys.label.frame;
    assert_eq!(lines[0].frame.y, keys.y + keys.height, "right under the entry's keys");
    assert!(lines.iter().all(|l| l.fill == Some(T.background)), "legible over the ink");

    assert_eq!(sling.key(Chord::new(0, '\r')), Some(Key::Send));
    assert!(sling.send());
    assert_eq!(sling.ready(|_| true).unwrap().prompt, "why?");
    let bare = Problem { message: "canvas \"notes\" not found".into(), hint: None };
    sling.answered(Err(bare));
    assert_eq!(texts(&sling), [("✗ canvas \"notes\" not found".into(), T.error)]);
    assert!(sling.cancel(), "escape gives up");
    assert_eq!(texts(&sling), []);
}

#[test]
fn send_now_skips_the_entry() {
    let mut sling = Sling::default();
    assert!(sling.send_now(&selection(), "from a button", BOX, PANE));
    assert!(!sling.is_typing());
    assert!(!sling.send_now(&selection(), "again", BOX, PANE), "one at a time");
    assert_eq!(sling.ready(|_| true).unwrap().prompt, "from a button");
    assert_eq!(sling.answered(Err(no_session())), Answered::Closed);
    assert_eq!((entry(&sling), texts(&sling)), (None, vec![]), "no entry for the pen to escape");
}

#[test]
fn dictation_s_lines_stand_under_the_entry_after_its_own() {
    let said = || vec![("✗ Flick is not running".to_string(), T.error), ("start".into(), T.muted)];
    assert!(Sling::default().under(&T, said()).is_empty(), "no entry: nowhere to show");
    let mut sling = typed("hi");
    let entry = entry(&sling).unwrap().frame;
    let ys = |sling: &Sling| -> Vec<f64> {
        sling.under(&T, said()).iter().map(|l| l.frame.y - entry.y).collect()
    };
    assert_eq!(ys(&sling), [HEIGHT + SMALL, HEIGHT + SMALL + LINE]);
    let lines = sling.under(&T, said());
    assert_eq!((lines[0].text.as_str(), lines[0].colour), ("✗ Flick is not running", T.error));
    assert!(sling.send());
    assert!(sling.under(&T, said()).is_empty(), "sent: the entry is gone");
    sling.ready(|_| true);
    sling.answered(Err(no_session()));
    let after = HEIGHT + SMALL + 2.0 * LINE;
    assert_eq!(ys(&sling), [after, after + LINE], "after its failure");
}

#[test]
fn a_text_field_keeps_its_edits_and_the_keymap_gets_the_rest() {
    use crate::commands::{ALT, CMD, CTRL, SHIFT};
    for (mods, key) in [
        (CMD, 'v'),
        (CMD, 'C'),
        (CMD | SHIFT, 'z'),
        (CMD, '\u{f702}'),
        (CMD | SHIFT, '\u{7f}'),
        (CTRL, 'a'),
        (ALT, '\u{f703}'),
        (0, 'q'),
    ] {
        assert!(edits(Chord::new(mods, key)), "{mods} {key:?} is the field's");
    }
    for (mods, key) in [(CMD, 'q'), (CMD, 'k'), (CMD, '\r'), (CMD | SHIFT, 'b'), (CTRL, '\t')] {
        assert!(!edits(Chord::new(mods, key)), "{mods} {key:?} is the keymap's");
    }
}

#[test]
fn the_selection_s_buttons_sling_prompt_and_delete_in_a_row_inside_the_pane() {
    use crate::commands::Command;
    use crate::scene::taps::Tap;
    let (labels, targets) = buttons::buttons(BOX, PANE, &T);
    let texts: Vec<&str> = labels.iter().map(|l| l.text.as_str()).collect();
    assert_eq!(texts, [buttons::SLING, buttons::PROMPT, buttons::DELETE]);
    let taps: Vec<Tap> = targets.iter().map(|t| t.1).collect();
    let runs = [Command::SlingNow, Command::Sling, Command::DeleteSelection].map(Tap::Run);
    assert_eq!(taps, runs);
    let frames: Vec<Rect> = targets.iter().map(|t| t.0).collect();
    assert!(
        frames.windows(2).all(|w| (w[0].y - w[1].y).abs() < 1e-9 && w[0].x + w[0].width < w[1].x)
    );
    assert!(frames[0].x > BOX.x + BOX.width, "right of the selection: {frames:?}");

    let corner = Rect { x: PANE.x + PANE.width - 20.0, ..BOX };
    let (_, targets) = buttons::buttons(corner, PANE, &T);
    let last = targets.last().unwrap().0;
    assert!(last.x + last.width <= PANE.x + PANE.width, "kept in the pane: {last:?}");
}
