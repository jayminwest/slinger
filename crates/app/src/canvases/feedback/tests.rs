use super::*;
use crate::backend::testing::{fake_flick, ready, user_stroke, wait_until};
use crate::backend::tests::{open_notes, with_store};
use crate::commands::Command;
use crate::commands::Keymap;
use crate::config::Config;
use crate::layout::WINDOW;
use crate::panes::{Kind, Pane};
use crate::scene::Scene;
use crate::scene::ink::Pen;
use crate::scene::sidebar::Sidebar;
use crate::scene::sling::pen::{CROSS, DICTATE, STOP};
use crate::scene::taps::{Tap, Target};

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

/// The labels and the targets of what the window draws for the sling
/// and the entries: here, the pen's buttons beside the open entry.
fn buttons(canvases: &Canvases, panes: &Panes) -> (Vec<String>, Vec<Target>) {
    let theme = Config::default().theme;
    let crate::scene::slung::Marks { labels, targets, .. } =
        canvases.sling_marks(panes, WINDOW.rect(), &theme);
    (labels.into_iter().map(|l| l.text).collect(), targets)
}

/// A tap with the pen on `target`.
fn tap(canvases: &mut Canvases, target: Target) {
    let (mut shared, mut sidebar) = (crate::shared::Shared::default(), Sidebar::new(false));
    crate::window::tap(target, canvases, &mut shared, &mut sidebar);
}

/// The canvases of the window, once the notes canvas has arrived, with
/// Flick on `socket`.
fn arrived(daemon: &Client, socket: &std::path::Path) -> (Canvases, Panes) {
    let (mut canvases, panes) = (Canvases::new(daemon.clone()), notes());
    canvases.listen_to(socket.to_path_buf());
    let theme = Config::default().theme;
    wait_until(|| {
        canvases.poll();
        !canvases.show(&panes, WINDOW.rect(), &theme).is_empty()
    });
    (canvases, panes)
}

/// slinger-a680b5: the pen's buttons beside a note's entry: "Dictate" has
/// Flick dictate into it (the sling prompt's way), "✓ Save" does what
/// return does.
#[test]
fn the_pen_dictates_a_note_and_saves_it_with_the_buttons_beside_its_entry() {
    let flick = tempfile::tempdir().unwrap();
    let (socket, heard) = fake_flick(flick.path(), ready);
    with_store(None, |daemon| {
        let _home = open_notes(daemon);
        user_stroke(daemon, "notes");
        let (mut canvases, panes) = arrived(daemon, &socket);
        let area = WINDOW.rect();
        assert_eq!(buttons(&canvases, &panes).0, Vec::<String>::new(), "no entry: no buttons");
        assert!(open(&mut canvases, &panes));
        let (labels, targets) = buttons(&canvases, &panes);
        assert_eq!(labels, ["✓ Save", DICTATE, CROSS]);
        let entry = shown(&canvases, &panes).entry.unwrap();
        assert!(targets.iter().all(|(f, _)| f.x > entry.frame.x + entry.frame.width));
        assert_eq!(targets[1].1, Tap::Run(Command::Dictate));
        assert!(canvases.sling_command(Command::Dictate, &panes, area), "the dictate button");
        assert_eq!(buttons(&canvases, &panes).0[1], STOP);
        let lines = shown(&canvases, &panes).labels;
        assert_eq!(lines[0].text, "● Flick is listening: dictate again when done");
        assert!(lines[0].frame.y > entry.frame.y + entry.frame.height, "under the note's box");
        wait_until(|| canvases.poll_dictation());
        assert!(canvases.sling_command(Command::Dictate, &panes, area), "again: stop");
        wait_until(|| canvases.poll_dictation());
        assert_eq!(buttons(&canvases, &panes).0[1], DICTATE);
        assert_eq!(*heard.lock().unwrap(), [r#"["dictation","start"]"#, r#"["dictation","stop"]"#]);
        // Flick types what it heard into the focused field, the note's.
        canvases.sling_typed("the pen lags");
        tap(&mut canvases, targets[0]);
        assert!(!canvases.typing(), "saving");
        wait_until(|| {
            canvases.poll();
            canvases.take_flash().is_some()
        });
        saved_with_its_context(daemon);
    });
}

/// slinger-a680b5: "✕" beside a new name's entry keeps the old name and
/// cancels its dictation; "✓ Rename" keeps the new one.
#[test]
fn the_pen_cancels_or_keeps_a_new_name_with_the_buttons_beside_its_entry() {
    let flick = tempfile::tempdir().unwrap();
    let (socket, heard) = fake_flick(flick.path(), ready);
    with_store(None, |daemon| {
        let _home = open_notes(daemon);
        user_stroke(daemon, "notes");
        let (mut canvases, panes) = arrived(daemon, &socket);
        let here = (panes.focus(), WINDOW.rect());
        assert!(canvases.command(&panes, here, Command::RenamePage));
        let (labels, targets) = buttons(&canvases, &panes);
        assert_eq!(labels, ["✓ Rename", DICTATE, CROSS]);
        assert!(canvases.sling_command(Command::Dictate, &panes, here.1));
        tap(&mut canvases, targets[2]);
        assert!(!canvases.typing(), "cancelled");
        wait_until(|| heard.lock().unwrap().len() == 2);
        assert_eq!(heard.lock().unwrap()[1], r#"["dictation","cancel"]"#, "and its dictation");

        assert!(canvases.command(&panes, here, Command::RenamePage));
        canvases.sling_typed("plans");
        let rename = buttons(&canvases, &panes).1[0];
        tap(&mut canvases, rename);
        assert!(!canvases.typing());
        wait_until(|| {
            canvases.poll();
            let named = daemon.canvas_show("notes", None).unwrap().pages;
            named.iter().any(|p| p["name"] == "plans")
        });
    });
}

/// slinger-e8d72a: every entry shows the dictate key beneath its box, and
/// the sling's prompt has the pen's buttons too: "✕" cancels it, "✓ Sling"
/// sends it.
#[test]
fn every_entry_shows_the_dictate_key_and_the_sling_s_has_the_pen_s_buttons() {
    let flick = tempfile::tempdir().unwrap();
    let (socket, _heard) = fake_flick(flick.path(), ready);
    with_store(None, |daemon| {
        let _home = open_notes(daemon);
        user_stroke(daemon, "notes");
        let (mut canvases, panes) = arrived(daemon, &socket);
        let here = (panes.focus(), WINDOW.rect());
        let key = Keymap::default().chord(Command::Dictate).unwrap();
        let dictate = |canvases: &Canvases| shown(canvases, &panes).entry.unwrap().dictate;
        assert!(open(&mut canvases, &panes));
        assert_eq!(dictate(&canvases).unwrap().label.text, format!("{} dictate", key.glyphs()));
        assert!(canvases.sling_key(ESCAPE));
        assert!(canvases.command(&panes, here, Command::RenamePage));
        assert!(dictate(&canvases).is_some(), "the new name's entry");
        assert!(canvases.sling_key(ESCAPE));

        lasso_the_stroke(&mut canvases, &panes);
        assert!(canvases.sling_command(Command::Sling, &panes, here.1));
        assert!(dictate(&canvases).is_some(), "the sling's");
        let (labels, targets) = buttons(&canvases, &panes);
        assert_eq!(labels, ["✓ Sling", DICTATE, CROSS]);
        let entry = shown(&canvases, &panes).entry.unwrap();
        let (pane, last) = (panes.place(here.1)[0].2, targets[2].0);
        assert!(targets[0].0.x > entry.frame.x + entry.frame.width, "right of the box");
        assert!(last.x + last.width <= pane.x + pane.width, "inside the pane");
        tap(&mut canvases, targets[2]);
        assert!(!canvases.typing(), "cancelled");
        sling_by_the_button(&mut canvases, &panes);
    });
}

/// The sling's entry opened again, "why?" typed, and "✓ Sling" tapped: it
/// goes and, with no agent running, fails and opens again with why.
fn sling_by_the_button(canvases: &mut Canvases, panes: &Panes) {
    assert!(canvases.sling_command(Command::Sling, panes, WINDOW.rect()));
    canvases.sling_typed("why?");
    let send = buttons(canvases, panes).1[0];
    tap(canvases, send);
    assert!(!canvases.typing(), "sent");
    wait_until(|| {
        canvases.poll();
        canvases.typing()
    });
    let lines = shown(canvases, panes).labels;
    assert!(lines[0].text.starts_with("✗ "), "{lines:?}");
}

/// A lasso round the stroke `user_stroke` made, with the pen.
fn lasso_the_stroke(canvases: &mut Canvases, panes: &Panes) {
    let at = (panes.focus(), WINDOW.rect());
    let here = Some(at);
    let pen =
        |x, y| Pen { at: Point { x, y }, when: 0.0, touch: false, eraser: false, shift: false };
    let lasso = (Tool::Lasso, Config::default().theme.text);
    assert!(canvases.pen_down(panes, at, pen(50.0, 50.0), lasso));
    for (x, y) in [(200.0, 50.0), (200.0, 200.0), (50.0, 200.0)] {
        canvases.pen_drag(panes, pen(x, y), here);
    }
    canvases.pen_up(panes, pen(50.0, 50.0), here);
    assert!(canvases.selection().is_some());
}
