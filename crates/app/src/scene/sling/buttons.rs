//! The pen's buttons at the selection (SPEC P6: the pen reaches every
//! common action with no keyboard): "➤ Sling" sends the selection at once
//! with no prompt, as the handwriting is the question, "+ prompt" opens
//! the entry to add words first, and "Delete" removes the selection (a tap
//! on empty canvas already selects nothing). They stand to the right of the selection's
//! box, level with its top (its count is above its top left), inside its
//! pane. Each is a tap target that runs its command when the pen lifts on
//! it ([`crate::scene::taps`]); a press there never draws ink.

use crate::commands::Command;
use crate::config::Theme;
use crate::layout::Rect;
use crate::scene::taps::{Tap, Target};
use crate::scene::{Label, style};

/// What the buttons say.
pub const SLING: &str = "➤ Sling";
pub const PROMPT: &str = "+ prompt";
pub const DELETE: &str = "Delete";
/// Each button's width; their height is a tap's.
const WIDE: f64 = 76.0;
/// The space between the selection's box (its handles) and the buttons,
/// and between the two.
const GAP: f64 = 8.0;
const BETWEEN: f64 = 4.0;

/// The buttons for a selection whose box is `around`, in its pane's frame
/// `within` (both in the window), in `theme`'s colours: their labels, and
/// their targets ("➤ Sling" runs `sling-now`, "+ prompt" runs `sling`,
/// "Delete" runs `delete-selection`).
pub fn buttons(around: Rect, within: Rect, theme: &Theme) -> (Vec<Label>, Vec<Target>) {
    let wide = 3.0 * WIDE + 2.0 * BETWEEN;
    let right = within.x + within.width - wide - GAP;
    let x = (around.x + around.width + GAP).min(right).max(within.x + GAP);
    let bottom = within.y + within.height - style::TAP - GAP;
    let y = around.y.min(bottom).max(within.y + GAP);
    let sling = Rect { x, y, width: WIDE, height: style::TAP };
    let prompt = Rect { x: x + WIDE + BETWEEN, ..sling };
    let delete = Rect { x: prompt.x + WIDE + BETWEEN, ..sling };
    let fill = Some(style::panel(theme));
    let labels = vec![
        Label {
            text: SLING.into(),
            colour: theme.background,
            fill: Some(theme.focus),
            frame: sling,
        },
        Label { text: PROMPT.into(), colour: theme.text, fill, frame: prompt },
        Label { text: DELETE.into(), colour: theme.text, fill, frame: delete },
    ];
    let targets = vec![
        (sling, Tap::Run(Command::SlingNow)),
        (prompt, Tap::Run(Command::Sling)),
        (delete, Tap::Run(Command::DeleteSelection)),
    ];
    (labels, targets)
}
