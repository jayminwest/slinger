use super::*;
use crate::backend::testing::{user_stroke, wait_until};
use crate::backend::tests::{open_notes, with_store};
use crate::commands::Keymap;
use crate::config::Config;
use crate::layout::WINDOW;
use crate::panes::{Kind, Pane};
use crate::scene::Scene;
use crate::scene::ink::Pen;
use crate::scene::sidebar::Sidebar;

const RETURN: Chord = Chord::new(0, '\r');
const ESCAPE: Chord = Chord::new(0, '\u{1b}');

fn notes() -> Panes {
    Panes::new(Pane { kind: Kind::Canvas, name: "notes".into() })
}

/// What the window shows over the panes: the entry and its lines.
fn shown(canvases: &Canvases, panes: &Panes) -> Scene {
    let theme = Config::default().theme;
    let side = Sidebar::new(false);
    let (uis, notes) = (crate::scene::Uis::new(), crate::scene::Notes::new());
    let mut scene =
        crate::scene::build(WINDOW, panes, &theme, &[], &side, Vec::new(), &uis, &notes);
    scene.labels.clear();
    canvases.show_sling(&mut scene, &theme, &Keymap::default(), &side);
    scene
}

/// Opens the entry in the focused pane of `panes`, with the pen.
fn open(canvases: &mut Canvases, panes: &Panes) -> bool {
    let at = (panes.focus(), WINDOW.rect());
    canvases.open_feedback(panes, at, Some("notes"), Tool::Pen)
}

#[test]
fn a_note_saves_with_the_canvas_page_and_tool_then_flashes() {
    with_store(None, |daemon| {
        let _home = open_notes(daemon);
        user_stroke(daemon, "notes");
        let (mut canvases, panes) = (Canvases::new(daemon.clone()), notes());
        let theme = Config::default().theme;
        wait_until(|| {
            canvases.poll();
            !canvases.show(&panes, WINDOW.rect(), &theme).is_empty()
        });

        assert!(open(&mut canvases, &panes));
        assert!(canvases.typing() && !canvases.open_sling(&panes, WINDOW.rect()), "one entry");
        let entry = shown(&canvases, &panes).entry.unwrap();
        assert_eq!(entry.title.label.text, "Feedback");
        let on = Point { x: entry.frame.x + 1.0, y: entry.frame.y + 1.0 };
        assert!(canvases.sling_covers(on));
        assert!(!canvases.sling_key(Chord::new(0, 'w')), "a letter is the entry's");
        assert!(!canvases.sling_typed("the pen lags"));
        assert!(!canvases.typed("x"), "not the canvas's");
        assert!(canvases.sling_key(RETURN));
        assert!(!canvases.typing(), "saving");
        assert_eq!(shown(&canvases, &panes).labels[0].text, "saving feedback…");

        let mut flash = None;
        wait_until(|| {
            canvases.poll();
            flash = canvases.take_flash();
            flash.is_some()
        });
        assert_eq!(flash.as_deref(), Some("Feedback saved"));
        let scene = shown(&canvases, &panes);
        assert_eq!((scene.entry, scene.labels.len()), (None, 0));
        saved_with_its_context(daemon);
        opens_listing_it(&mut canvases, &panes);
    });
}

/// Opening the entry again lists the note saved under it.
fn opens_listing_it(canvases: &mut Canvases, panes: &Panes) {
    assert!(open(canvases, panes));
    wait_until(|| canvases.poll_feedback());
    let lines: Vec<_> = shown(canvases, panes).labels.into_iter().map(|l| l.text).collect();
    assert_eq!(lines.len(), 2, "{lines:?}");
    assert!(lines[0] == "Recent feedback" && lines[1].ends_with(" · the pen lags"), "{lines:?}");
}

/// The daemon holds the note typed, with what the pane showed.
fn saved_with_its_context(daemon: &Client) {
    let saved = daemon.feedback_list(false, None).unwrap().entries;
    let note = &saved[0];
    assert_eq!(note.text, "the pen lags");
    let (workspace, canvas) = (note.workspace.as_deref(), note.canvas.as_deref());
    assert_eq!((workspace, canvas), (Some("notes"), Some("notes")));
    assert_eq!((note.tool.as_deref(), note.selected), (Some("pen"), None));
    assert!(note.page.is_some(), "the page's name");
}

#[test]
fn escape_cancels_and_a_lost_daemon_opens_the_entry_again() {
    let mut canvases = Canvases::new(Client::new("/nonexistent/x.sock"));
    let panes = notes();
    assert!(open(&mut canvases, &panes));
    assert!(canvases.sling_key(ESCAPE));
    assert!(!canvases.typing() && !canvases.sling_key(ESCAPE), "closed: the keymap's");

    assert!(open(&mut canvases, &panes));
    let pen =
        Pen { at: Point { x: 9.0, y: 9.0 }, when: 0.0, touch: true, eraser: false, shift: false };
    let (here, ink) = ((panes.focus(), WINDOW.rect()), (Tool::Pen, Config::default().theme.text));
    canvases.pen_down(&panes, here, pen, ink);
    assert!(canvases.typing(), "a palm on the canvas leaves it open");
    canvases.pen_down(&panes, here, Pen { touch: false, ..pen }, ink);
    assert!(!canvases.typing(), "a press on the canvas closes it");

    assert!(open(&mut canvases, &panes));
    canvases.sling_typed("slow");
    assert!(canvases.sling_key(RETURN));
    assert!(!open(&mut canvases, &panes), "saving");
    wait_until(|| canvases.poll_feedback());
    let scene = shown(&canvases, &panes);
    assert_eq!(scene.entry.map(|e| e.text), Some("slow".into()));
    assert!(scene.labels[0].text.starts_with("✗ "), "{:?}", scene.labels);
    assert_eq!(canvases.take_flash(), None);
}
