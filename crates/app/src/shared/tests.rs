use slinger_control::{Pane as Shown, PaneKind, PaneSplit, PaneTree};

use super::*;
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
