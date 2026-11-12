//! The pen's buttons beside an open entry (the sling's prompt, a page's new
//! name, a note of feedback; SPEC P6: the pen reaches every common action
//! with no keyboard): "✓ Sling", "✓ Rename" or "✓ Save" does what return
//! does, "Dictate" starts Flick's dictation into the entry, the
//! way the dictate key does, and "Stop" ends it, and "✕" does what escape does.
//! They stand to the right of the entry's box, level with it; the box
//! makes room for them ([`beside`]). Each is a tap target that acts when
//! the pen lifts on it ([`crate::scene::taps`]).

use super::{GAP, HEIGHT, WIDTH, centred};
use crate::commands::Command;
use crate::config::Theme;
use crate::layout::Rect;
use crate::scene::taps::{Tap, Target};
use crate::scene::{Label, style};

/// What the dictate button says, idle and while Flick listens, and the
/// cancel button.
pub const DICTATE: &str = "Dictate";
pub const STOP: &str = "● Stop";
pub const CROSS: &str = "✕";
/// The buttons' widths; their height is a tap's.
const DONE_WIDE: f64 = 84.0;
const DICTATE_WIDE: f64 = 68.0;
/// The space between the box and the buttons, and between two buttons.
const BETWEEN: f64 = 4.0;
/// The room the buttons take beside the box.
pub const ROOM: f64 = GAP + DONE_WIDE + DICTATE_WIDE + style::TAP + 2.0 * BETWEEN;

/// An entry's box in `within` (a pane) with its buttons beside it: as
/// [`centred`], narrower by the buttons' room when the pane is, the box
/// and the buttons centred across the pane together.
pub fn beside(within: Rect) -> Rect {
    let width = WIDTH.min(within.width - 2.0 * GAP - ROOM).max(0.0);
    let x = within.x + (within.width - width - ROOM) / 2.0;
    Rect { x: x.max(within.x), width, ..centred(within) }
}

/// The buttons beside the entry's box `frame`, in `theme`'s colours: "✓
/// `done`" sends ([`Tap::Send`]), the dictate button runs `dictate`
/// ("● Stop" while `listening`), "✕" cancels ([`Tap::Cancel`]). Their
/// labels and their targets.
pub fn buttons(
    frame: Rect,
    done: &str,
    listening: bool,
    theme: &Theme,
) -> (Vec<Label>, Vec<Target>) {
    let y = frame.y + (HEIGHT - style::TAP) / 2.0;
    let send = Rect { x: frame.x + frame.width + GAP, y, width: DONE_WIDE, height: style::TAP };
    let dictate = Rect { x: send.x + send.width + BETWEEN, width: DICTATE_WIDE, ..send };
    let cancel = Rect { x: dictate.x + dictate.width + BETWEEN, width: style::TAP, ..send };
    let fill = Some(style::panel(theme));
    let labels = vec![
        Label {
            text: format!("✓ {done}"),
            colour: theme.background,
            fill: Some(theme.focus),
            frame: send,
        },
        Label {
            text: (if listening { STOP } else { DICTATE }).into(),
            colour: theme.text,
            fill,
            frame: dictate,
        },
        Label { text: CROSS.into(), colour: theme.text, fill, frame: cancel },
    ];
    let targets =
        vec![(send, Tap::Send), (dictate, Tap::Run(Command::Dictate)), (cancel, Tap::Cancel)];
    (labels, targets)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PANE: Rect = Rect { x: 100.0, y: 0.0, width: 800.0, height: 600.0 };

    #[test]
    fn the_buttons_stand_right_of_the_box_inside_the_pane_and_do_return_dictate_and_escape() {
        let theme = Theme::default();
        let frame = beside(PANE);
        assert_eq!((frame.width, frame.y), (WIDTH, centred(PANE).y), "a wide pane: the full box");
        let (labels, targets) = buttons(frame, "Save", false, &theme);
        let texts: Vec<&str> = labels.iter().map(|l| l.text.as_str()).collect();
        assert_eq!(texts, ["✓ Save", DICTATE, CROSS]);
        let taps: Vec<Tap> = targets.iter().map(|t| t.1).collect();
        assert_eq!(taps, [Tap::Send, Tap::Run(Command::Dictate), Tap::Cancel]);
        let last = targets[2].0;
        assert!(targets[0].0.x > frame.x + frame.width, "right of the box");
        let (left, right) = (frame.x - PANE.x, PANE.x + PANE.width - (last.x + last.width));
        assert!((left - right).abs() < 0.01, "centred together: {left} {right}");
        for (target, label) in targets.iter().zip(&labels) {
            assert_eq!(target.0, label.frame);
            assert!(target.0.y >= frame.y && target.0.y + target.0.height <= frame.y + HEIGHT);
        }
        assert_eq!(buttons(frame, "Save", true, &theme).0[1].text, STOP, "while Flick listens");
    }

    #[test]
    fn a_narrow_pane_narrows_the_box_and_keeps_the_buttons_inside() {
        let pane = Rect { width: 420.0, ..PANE };
        let frame = beside(pane);
        assert!(frame.width < WIDTH && frame.width > 0.0, "{frame:?}");
        let (_, targets) = buttons(frame, "Rename", false, &Theme::default());
        let last = targets[2].0;
        assert!(frame.x >= pane.x + GAP - 0.01 && last.x + last.width <= pane.x + pane.width - GAP);
    }
}
