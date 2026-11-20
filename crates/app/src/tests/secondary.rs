//! The right and middle buttons (slinger-f32672): a drag past the slop
//! pans as a two-finger scroll does, a right click pans nothing and focuses
//! the pane under it, and a press over an open entry goes on to its field.

use super::*;
use crate::layout::{PaneId, Viewport};
use crate::scene::secondary::{Button, Phase};

fn button(button: Button, phase: Phase, x: f64, y: f64) -> Event {
    Event::Button { button, phase, at: Point { x, y } }
}

/// The viewport each canvas pane draws now, and the items on the daemon.
fn seen(w: &mut World, daemon: &control::Client) -> (Vec<(PaneId, Viewport)>, usize) {
    let area = w.sidebar.panes(w.shared.size.rect());
    let theme = w.settings.config.theme;
    let panes = w.canvases.show(&w.shared.panes, area, &theme);
    let views = panes.iter().map(|p| (p.pane, p.scene.viewport)).collect();
    (views, daemon.canvas_show("notes", None).unwrap().items.len())
}

/// A press of `which` at 300,300, drags to each of `to` and a lift at the last.
fn drag(w: &mut World, which: Button, to: &[(f64, f64)]) {
    assert!(w.send(button(which, Phase::Down, 300.0, 300.0)), "the app takes the press");
    for &(x, y) in to {
        w.send(button(which, Phase::Drag, x, y));
    }
    let &(x, y) = to.last().unwrap();
    w.send(button(which, Phase::Up, x, y));
}

#[test]
fn a_right_or_middle_drag_pans_as_the_same_scroll_does() {
    with_store(None, |daemon| {
        let _home = open_notes(daemon);
        let config = tempfile::tempdir().unwrap();
        let mut scrolled = world(daemon, config.path());
        let start = seen(&mut scrolled, daemon);
        let at = Point { x: 300.0, y: 300.0 };
        assert!(scrolled.send(Event::Scroll { at, dx: 30.0, dy: -20.0 }));
        let want = seen(&mut scrolled, daemon);
        assert_ne!(want, start);

        for which in [Button::Secondary, Button::Middle] {
            let mut w = world(daemon, config.path());
            drag(&mut w, which, &[(302.0, 299.0), (320.0, 290.0), (330.0, 280.0)]);
            assert_eq!(seen(&mut w, daemon), want, "{which:?}: the whole way, no more");
        }
    });
}

#[test]
fn a_right_click_pans_nothing_draws_nothing_and_focuses_the_pane_under_it() {
    with_store(None, |daemon| {
        let _home = open_notes(daemon);
        let config = tempfile::tempdir().unwrap();
        let mut w = world(daemon, config.path());
        let left = w.shared.panes.focus();
        assert!(w.send(Event::Command(Command::SplitRight)));
        assert_ne!(w.shared.panes.focus(), left, "the new pane has focus");
        let before = seen(&mut w, daemon);
        drag(&mut w, Button::Secondary, &[(302.0, 301.0)]);
        assert_eq!(w.shared.panes.focus(), left, "the click focused the pane under it");
        assert_eq!(seen(&mut w, daemon), before, "no pan, no item");

        // In the right pane.
        w.send(button(Button::Middle, Phase::Down, 800.0, 300.0));
        w.send(button(Button::Middle, Phase::Up, 800.0, 300.0));
        assert_eq!(w.shared.panes.focus(), left, "the middle button never clicks");
    });
}

#[test]
fn a_lift_or_a_drag_with_no_press_goes_on() {
    with_store(None, |daemon| {
        let _home = open_notes(daemon);
        let config = tempfile::tempdir().unwrap();
        let mut w = world(daemon, config.path());
        assert!(!w.send(button(Button::Secondary, Phase::Drag, 300.0, 300.0)));
        assert!(!w.send(button(Button::Secondary, Phase::Up, 300.0, 300.0)));
    });
}

#[test]
fn a_right_press_on_an_open_entry_goes_on_to_its_field() {
    with_store(None, |daemon| {
        let _home = open_notes(daemon);
        let config = tempfile::tempdir().unwrap();
        let mut w = world(daemon, config.path());
        assert!(w.send(Event::Command(Command::LeaveFeedback)));
        let scene = view(&w.settings, &mut w.shared, &w.tools, &w.sidebar, &mut w.canvases);
        let frame = scene.entry.expect("the feedback entry is open").frame;
        let (x, y) = (frame.x + 4.0, frame.y + 4.0);
        assert!(!w.send(button(Button::Secondary, Phase::Down, x, y)), "the field's own menu");
        assert!(!w.send(button(Button::Secondary, Phase::Up, x, y)), "its lift too");
        assert!(w.canvases.typing(), "the entry is still open");
    });
}
