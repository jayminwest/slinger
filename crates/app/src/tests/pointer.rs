//! The pointer, per docs/design/pen-less.md pick 1 (slinger-9c9553): with
//! the pen tool, a mouse press selects and a tablet press draws; space held
//! turns a drag into a pan.

use super::*;
use crate::layout::Viewport;
use crate::scene::cursor::Cursor;

/// A mouse or trackpad event at `x`, `y`.
fn mouse(x: f64, y: f64, when: f64) -> Pen {
    Pen { tablet: false, ..at(x, y, when) }
}

/// A trackpad event at `x`, `y`: `AppKit` says a touch made it.
fn trackpad(x: f64, y: f64, when: f64) -> Pen {
    Pen { touch: true, ..mouse(x, y, when) }
}

/// A press, a drag and a lift by `pen` from `x`, each used.
fn stroke(w: &mut World, pen: fn(f64, f64, f64) -> Pen, x: f64, when: f64) {
    assert!(w.send(Event::Press(pen(x, 300.0, when))));
    assert!(w.send(Event::Drag(pen(x + 40.0, 320.0, when + 0.1))));
    assert!(w.send(Event::Drag(pen(x + 80.0, 340.0, when + 0.2))));
    assert!(w.send(Event::Lift(pen(x + 80.0, 340.0, when + 0.3))));
}

/// The focused pane's viewport and cursor as drawn now.
fn shown(w: &mut World) -> (Viewport, Cursor) {
    let scene = view(&w.settings, &mut w.shared, &w.tools, &w.sidebar, &mut w.canvases);
    let pane = &scene.canvases[0];
    (pane.scene.viewport, pane.cursor)
}

#[test]
fn with_the_pen_tool_a_mouse_drag_draws_no_line_and_a_tablet_drag_draws() {
    with_store(None, |daemon| {
        let _home = open_notes(daemon);
        let config = tempfile::tempdir().unwrap();
        let mut w = world(daemon, config.path());
        stroke(&mut w, mouse, 300.0, 1.0);
        assert_eq!((w.tools.tool(), w.tools.chosen()), (Tool::Select, Tool::Pen));
        assert_eq!(shown(&mut w).1, Cursor::Arrow, "the select tool's pointer");
        stroke(&mut w, at, 300.0, 2.0);
        assert_eq!(w.tools.tool(), Tool::Pen, "the tablet's press draws");
        w.until(|_| count(daemon, "stroke") == 1);
        assert_eq!(count(daemon, "stroke"), 1, "the mouse's drag drew nothing before it");
        assert!(w.send(Event::Proximity { near: false, eraser: false }), "the pen left");
        assert_eq!(w.tools.tool(), Tool::Select);
        assert!(w.send(Event::Proximity { near: true, eraser: false }), "it came back");
        assert_eq!(w.tools.tool(), Tool::Pen);
    });
}

#[test]
fn a_trackpad_press_is_a_mouse_press_and_a_palm_only_while_the_pen_is_near() {
    with_store(None, |daemon| {
        let _home = open_notes(daemon);
        let config = tempfile::tempdir().unwrap();
        let mut w = world(daemon, config.path());
        assert!(w.send(Event::Command(Command::ToolBox)));
        stroke(&mut w, trackpad, 300.0, 1.0);
        w.until(|_| count(daemon, "box") == 1);
        assert!(w.send(Event::Proximity { near: true, eraser: false }), "the pen came near");
        assert!(!w.send(Event::Press(trackpad(500.0, 300.0, 2.0))), "a palm: nothing starts");
        w.send(Event::Drag(trackpad(580.0, 340.0, 2.1)));
        w.send(Event::Lift(trackpad(580.0, 340.0, 2.2)));
        stroke(&mut w, at, 300.0, 3.0);
        w.until(|_| count(daemon, "box") == 2);
        assert_eq!(count(daemon, "box"), 2, "the palm drew none between them");
    });
}

#[test]
fn space_held_pans_the_pane_under_a_drag_with_a_hand_and_draws_nothing() {
    with_store(None, |daemon| {
        let _home = open_notes(daemon);
        let config = tempfile::tempdir().unwrap();
        let mut w = world(daemon, config.path());
        let (start, _) = shown(&mut w);
        assert!(w.send(Event::Space(true)), "nothing takes text: the hand's");
        assert!(w.send(Event::Space(true)), "a repeat types nothing either");
        assert_eq!(shown(&mut w), (start, Cursor::OpenHand));
        stroke(&mut w, at, 300.0, 1.0);
        let (panned, cursor) = shown(&mut w);
        assert_ne!(panned, start, "the drag panned");
        assert_eq!(cursor, Cursor::OpenHand, "space is still held");
        assert!(w.send(Event::Space(false)));
        assert_ne!(shown(&mut w).1, Cursor::OpenHand);
        assert!(!w.send(Event::Space(false)), "a space up with none held goes on");

        // The pan drew nothing: the next stroke is the canvas's first.
        stroke(&mut w, at, 300.0, 3.0);
        w.until(|_| count(daemon, "stroke") == 1);
        assert_eq!(count(daemon, "stroke"), 1);
    });
}

#[test]
fn space_types_in_a_text_being_typed_and_does_not_pan() {
    with_store(None, |daemon| {
        let _home = open_notes(daemon);
        let config = tempfile::tempdir().unwrap();
        let mut w = world(daemon, config.path());
        w.type_word(300.0, 1.0);
        assert!(!w.send(Event::Space(true)), "a text takes typing: the space goes on");
        assert!(w.send(Event::Text(" ".into())));
        assert!(w.canvases.editing());
        assert_ne!(shown(&mut w).1, Cursor::OpenHand);
    });
}
