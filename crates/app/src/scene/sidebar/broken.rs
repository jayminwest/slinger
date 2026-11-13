//! A workspace whose file the daemon cannot use, as the sidebar shows it:
//! under its name, after a "✗", why (the error's message, less the file's
//! path), the field that broke a rule and the hint, each wrapped to the
//! sidebar's width. Every verb on it fails, so it shows no agent.

use super::{Line, MARK, PAD, WIDTH};
use crate::config::Theme;
use crate::layout::Point;
use crate::scene::{glyphs, slung};

/// The size of the sidebar's plain text: the system's.
const TEXT: f64 = 13.0;

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
    /// colour after a "✗", then the field and the hint, muted.
    pub(super) fn lines(&self, theme: &Theme) -> Vec<Line> {
        let room = WIDTH - 2.0 * PAD - MARK;
        let at = Point { x: 0.0, y: 0.0 };
        let fits = |line: &str| glyphs::frame(at, line, TEXT, glyphs::UNWRAPPED).width <= room;
        let field = self.field.as_ref().map(|f| format!("field: {f}"));
        let (error, muted) = (theme.error, theme.muted);
        let said =
            [(Some(&self.message), error), (field.as_ref(), muted), (self.hint.as_ref(), muted)];
        let mut lines = Vec::new();
        for (text, colour) in said {
            for line in text.map(|t| slung::wrap(t, &fits)).unwrap_or_default() {
                lines.push(Line::under(line, colour));
            }
        }
        if let Some(first) = lines.first_mut() {
            first.mark = Some(("✗", theme.error));
        }
        lines
    }
}
