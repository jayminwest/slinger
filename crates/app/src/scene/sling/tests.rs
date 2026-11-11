use super::*;
use crate::config::Rgb;

const T: Theme = Theme {
    background: Rgb { r: 0, g: 0, b: 0 },
    text: Rgb { r: 1, g: 1, b: 1 },
    muted: Rgb { r: 2, g: 2, b: 2 },
    error: Rgb { r: 3, g: 3, b: 3 },
    focus: Rgb { r: 4, g: 4, b: 4 },
};

const PANE: Rect = Rect { x: 100.0, y: 0.0, width: 600.0, height: 400.0 };
const BOX: Rect = Rect { x: 200.0, y: 100.0, width: 50.0, height: 40.0 };

fn selection() -> Selection {
    Selection { canvas: "notes".into(), page: "p1".into(), ids: vec![ItemId(1), ItemId(4)] }
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
    let entry = sling.entry().unwrap();
    assert_eq!(entry.text, "");
    assert_eq!(entry.placeholder, "sling 2 items to the agent: what to ask (or nothing)");
    assert_eq!(entry.frame, Rect { x: 200.0, y: 148.0, width: WIDTH, height: HEIGHT });
    assert!(sling.is_typing());
    assert!(sling.covers(Point { x: 210.0, y: 150.0 }));
    assert!(!sling.covers(Point { x: 210.0, y: 120.0 }), "the selection is not the entry");
    assert_eq!(texts(&sling), []);
}

#[test]
fn the_entry_stays_inside_its_pane() {
    let low = Rect { y: 360.0, ..BOX };
    assert_eq!(place(low, PANE).y, 360.0 - GAP - HEIGHT, "above when no room below");
    let right = Rect { x: 680.0, ..BOX };
    assert_eq!(place(right, PANE).x, 700.0 - WIDTH - GAP);
    let left = Rect { x: 0.0, ..BOX };
    assert_eq!(place(left, PANE).x, 100.0 + GAP);
    let tall = Rect { y: 10.0, height: 390.0, ..BOX };
    assert_eq!(place(tall, PANE).y, GAP, "never above the pane");
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
    assert_eq!(sling.entry().unwrap().text, "what is missing?");
    assert!(sling.send());
    assert!(!sling.is_typing() && sling.entry().is_none());
    sling.typed("ignored once sent");
    assert_eq!(texts(&sling), [("slinging 2 items: what is missing?".into(), T.muted)]);
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
    assert!(sling.answered(Ok(())));
    assert_eq!((sling.entry(), texts(&sling)), (None, vec![]), "it went: nothing more shows");
    assert!(!sling.answered(Ok(())), "no sling on its way");
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
    assert_eq!((sling.entry(), texts(&sling)), (None, vec![]));
    assert_eq!(sling.ready(|_| true), None);
    assert!(!sling.cancel() && !sling.send(), "closed: nothing to cancel or send");
    assert!(sling.open(&selection(), BOX, PANE));
    assert_eq!(sling.entry().unwrap().text, "", "a new sling starts empty");
}

#[test]
fn opening_again_moves_the_entry_and_keeps_the_text() {
    let mut sling = typed("half a thought");
    let one = Selection { ids: vec![ItemId(9)], ..selection() };
    assert!(sling.open(&one, Rect { x: 300.0, ..BOX }, PANE));
    let entry = sling.entry().unwrap();
    assert_eq!((entry.text.as_str(), entry.frame.x), ("half a thought", 300.0));
    assert!(entry.placeholder.starts_with("sling 1 item to"));
}

#[test]
fn a_failure_shows_its_message_and_hint_under_the_entry_and_return_tries_again() {
    let mut sling = typed("why?");
    sling.send();
    sling.ready(|_| true).unwrap();
    assert!(sling.answered(Err(no_session())));
    assert!(sling.is_typing());
    let entry = sling.entry().unwrap();
    assert_eq!(entry.text, "why?", "the prompt is kept");
    assert_eq!(
        texts(&sling),
        [
            ("✗ scout is not running".into(), T.error),
            ("start it with 'slinger session start notes'".into(), T.muted),
        ]
    );
    let lines = sling.labels(&T);
    assert_eq!(lines[0].frame.y, entry.frame.y + entry.frame.height, "right under the entry");
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
    sling.answered(Err(no_session()));
    assert_eq!(sling.entry().unwrap().text, "from a button", "a failure opens the entry");
}

#[test]
fn dictation_s_lines_stand_under_the_entry_after_its_own() {
    let said = || vec![("✗ Flick is not running".to_string(), T.error), ("start".into(), T.muted)];
    assert!(Sling::default().under(&T, said()).is_empty(), "no entry: nowhere to show");
    let mut sling = typed("hi");
    let entry = sling.entry().unwrap().frame;
    let ys = |sling: &Sling| -> Vec<f64> {
        sling.under(&T, said()).iter().map(|l| l.frame.y - entry.y).collect()
    };
    assert_eq!(ys(&sling), [HEIGHT, HEIGHT + LINE]);
    let lines = sling.under(&T, said());
    assert_eq!((lines[0].text.as_str(), lines[0].colour), ("✗ Flick is not running", T.error));
    assert!(sling.send());
    assert!(sling.under(&T, said()).is_empty(), "sent: the entry is gone");
    sling.ready(|_| true);
    sling.answered(Err(no_session()));
    assert_eq!(ys(&sling), [HEIGHT + 2.0 * LINE, HEIGHT + 3.0 * LINE], "after its failure");
}
