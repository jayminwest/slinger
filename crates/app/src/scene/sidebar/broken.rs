//! A workspace whose file the daemon cannot use, as the sidebar shows it:
//! under its name, after a "✗", why (the error's message, less the file's
//! path) and the field that broke a rule; while the window shows it, a
//! line whose tap closes it (`close-workspace`), then the hint. Each is
//! wrapped to the sidebar's width and cut short with "…" past a few lines,
//! so a long one never pushes the rows under it past the sidebar's foot
//! (slinger-a57147): a tap on it shows it, wider, in the panes' area. Every
//! verb on it fails, so it shows no agent.

use super::{Line, MARK, PAD, WIDTH};
use crate::commands::Command;
use crate::config::Theme;
use crate::layout::Point;
use crate::scene::{glyphs, slung};

/// The size of the sidebar's plain text: the system's.
const TEXT: f64 = 13.0;
/// The close line's text.
pub const CLOSE: &str = "Close workspace";
/// The most lines the message, the field and the hint each take.
const MOST: [usize; 3] = [2, 1, 3];
/// What ends a line cut short.
pub const CUT: &str = "…";
/// How far in from each side of its row the text field that draws a line
/// (an `NSTextField`'s cell) sets its text: a line wrapped to the whole row
/// loses its last word (slinger-c19909).
pub(super) const INSET: f64 = 2.0;

/// Why the daemon cannot use a workspace's file: its error.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Broken {
    pub message: String,
    pub hint: Option<String>,
    /// The field that broke a rule (`agent.tools`), when one did.
    pub field: Option<String>,
}

impl Broken {
    /// Its lines under the workspace's name: the message in the error
    /// colour after a "✗", then the field, muted; then, when `shown`, the
    /// close line and the hint (muted). The hint shows only then: the
    /// close line does what it says, and the panes show it wider.
    pub(super) fn lines(&self, shown: bool, theme: &Theme) -> Vec<Line> {
        let room = WIDTH - 2.0 * PAD - MARK - 2.0 * INSET;
        let at = Point { x: 0.0, y: 0.0 };
        let fits = |line: &str| glyphs::frame(at, line, TEXT).width <= room;
        let field = self.field.as_ref().map(|f| format!("field: {f}"));
        let (error, muted) = (theme.error, theme.muted);
        let hint = self.hint.as_ref().filter(|_| shown);
        let said = [(Some(&self.message), error), (field.as_ref(), muted), (hint, muted)];
        let mut lines = Vec::new();
        for (i, (text, colour)) in said.into_iter().enumerate() {
            if i == 2 && shown {
                let close = Line::under(CLOSE.into(), theme.focus);
                lines.push(Line { run: Some(Command::CloseWorkspace), ..close });
            }
            for line in text.map(|t| clamped(t, MOST[i], &fits)).unwrap_or_default() {
                lines.push(Line::under(line, colour));
            }
        }
        if let Some(first) = lines.first_mut() {
            first.mark = Some(("✗", theme.error));
        }
        lines
    }
}

/// `text` wrapped to lines that each `fits`, at most `most` of them (one
/// at least): when some are left out, the last kept ends in [`CUT`].
fn clamped(text: &str, most: usize, fits: &dyn Fn(&str) -> bool) -> Vec<String> {
    let mut lines = slung::wrap(text, fits);
    if lines.len() > most.max(1) {
        lines.truncate(most.max(1));
        if let Some(last) = lines.last_mut() {
            while !last.is_empty() && !fits(&format!("{}{CUT}", last.trim_end())) {
                last.pop();
            }
            *last = format!("{}{CUT}", last.trim_end());
        }
    }
    lines
}
