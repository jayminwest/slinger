use slinger_control::{Pane as Shown, PaneKind, PaneSplit, PaneTree};

use super::*;
use crate::scene::taps::Tap;
use crate::window::focus;

const AREA: Rect = Rect { x: 0.0, y: 0.0, width: 800.0, height: 600.0 };

/// Two canvas panes side by side, focus on the first, as the server sends them.
fn two() -> Shared {
    let leaf = |pane| PaneTree::Pane(Shown { pane, kind: PaneKind::Canvas, target: "c".into() });
    let tree = PaneTree::Split {
        split: PaneSplit::Row,
        ratio: 0.5,
        first: leaf(1).into(),
        second: leaf(2).into(),
    };
    Shared { panes: Panes::follow(&tree, 1), ..Shared::default() }
}

#[test]
fn a_press_on_the_other_pane_focuses_it_and_its_commands_follow() {
    let mut shared = two();
    let first = shared.panes.focus();
    let in_b = Point { x: 600.0, y: 200.0 };
    assert!(shared.press_focus(AREA, in_b, false));
    let (pane, frame) = focus(&shared.panes, AREA);
    assert_ne!(pane, first);
    assert_eq!(frame.x, 400.0, "the focused pane's commands act on the pressed one");
    assert_eq!(shared.panes.number(), 2, "a split or a close names it to the server");
    let footer = Point { x: 100.0, y: 590.0 };
    assert!(shared.press_focus(AREA, footer, false), "a press on a footer focuses its pane");
    assert_eq!(shared.panes.focus(), first);
}

#[test]
fn a_touch_or_a_press_under_the_key_overlay_or_palette_does_not_focus() {
    let mut shared = two();
    let in_b = Point { x: 600.0, y: 200.0 };
    assert!(!shared.press_focus(AREA, in_b, true), "a palm on the trackpad");
    shared.overlay.toggle();
    assert!(!shared.press_focus(AREA, in_b, false));
    shared.overlay.toggle();
    shared.palette.toggle();
    assert!(!shared.press_focus(AREA, in_b, false));
    shared.palette.toggle();
    assert!(shared.press_focus(AREA, in_b, false));
}

#[test]
fn a_press_on_a_button_is_its_alone_and_the_lift_on_it_taps_it() {
    let mut shared = two();
    let undo = Rect { x: 10.0, y: 572.0, width: 28.0, height: 28.0 };
    shared.taps.set(vec![(undo, Tap::Run(Command::Undo))]);
    let on = Point { x: 20.0, y: 580.0 };
    assert!(shared.hover(on), "the button under the pen lights");
    assert_eq!(shared.press(on), Took::Yes, "no ink, no gesture");
    assert!(shared.drag(Point { x: 22.0, y: 581.0 }), "the drag is the button's");
    assert_eq!(shared.lift(on), Took::Tap((undo, Tap::Run(Command::Undo))));
    let away = Point { x: 300.0, y: 300.0 };
    assert_eq!(shared.press(away), Took::No, "the canvas has it");
    assert!(!shared.drag(away));
    assert_eq!(shared.lift(on), Took::No);
    shared.overlay.toggle();
    assert_eq!(shared.press(on), Took::Yes, "the key overlay has every press: it closes");
    assert_eq!(shared.lift(on), Took::No, "and the button was never held");
}

#[test]
fn the_open_page_menu_takes_every_press_and_its_rows_tap() {
    let mut shared = two();
    let anchor = Rect { x: 150.0, y: 572.0, width: 100.0, height: 28.0 };
    let pane = shared.panes.focus();
    shared.menu.open(pane, anchor, (vec!["a".into(), "b".into()], 0));
    assert!(shared.modal() && shared.scroll(4.0));
    assert!(!shared.press_focus(AREA, Point { x: 600.0, y: 200.0 }, false), "focus stays");
    // The commands' rows sit at the panel's foot, just above the page name.
    let remove = Point { x: 200.0, y: anchor.y - 12.0 };
    assert_eq!(shared.press(remove), Took::Yes);
    assert!(shared.drag(remove));
    let Took::Tap((_, tap)) = shared.lift(remove) else { panic!("a tap") };
    assert_eq!(tap, Tap::Run(Command::RemovePage));
    assert!(!shared.modal(), "a pick closes it");
    shared.menu.open(pane, anchor, (vec!["a".into()], 0));
    assert_eq!(shared.press(Point { x: 700.0, y: 100.0 }), Took::Yes, "outside: it closes");
    assert!(!shared.modal());
}
