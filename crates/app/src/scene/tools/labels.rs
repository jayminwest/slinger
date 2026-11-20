//! The open pop-up drawn as labels ([`Tools::labels`]).

use super::{Kind, SHAPES, SIZES, TOOLS, Tools};
use crate::config::{Rgb, Theme, Width};
use crate::layout::Rect;
use crate::scene::Label;

impl Tools {
    /// The open pop-up as labels in `theme`'s colours, to draw on top of
    /// everything else: a panel with a border, then each cell. A colour is
    /// a swatch, with a ring when lit; a word (a tool, or a size in points)
    /// is lit by swapping its colours; a width is a line that wide in the
    /// pen's colour, framed when lit.
    pub fn labels(&self, theme: &Theme) -> Vec<Label> {
        let Some(popup) = &self.popup else { return Vec::new() };
        let fill = |frame: Rect, colour: Rgb| Label {
            text: String::new(),
            colour,
            fill: Some(colour),
            frame,
        };
        let panel = popup.place.panel;
        let mut labels = vec![fill(panel, theme.muted), fill(panel.outset(-1.0), theme.background)];
        for (i, &cell) in popup.place.cells.iter().enumerate() {
            let lit = i == popup.lit;
            let word = |text: String| Label {
                text,
                colour: if lit { theme.background } else { theme.text },
                fill: lit.then_some(theme.text),
                frame: cell,
            };
            match popup.kind {
                Kind::Colour => {
                    if lit {
                        labels.push(fill(cell.outset(2.0), theme.text));
                    }
                    labels.push(fill(cell, self.palette.colours()[i]));
                }
                Kind::Shape => labels.push(word(SHAPES[i].name().to_string())),
                Kind::Tool => labels.push(word(TOOLS[i].name().to_string())),
                Kind::Size => labels.push(word(SIZES[i].to_string())),
                Kind::Width => {
                    if lit {
                        labels.push(fill(cell, theme.text));
                        labels.push(fill(cell.outset(-1.0), theme.background));
                    }
                    labels.push(fill(line(cell, self.widths.widths()[i]), self.colour));
                }
            }
        }
        labels
    }
}

/// A line `width` wide across the middle of `cell`, inside its edges: as
/// wide as the cell leaves room for, at least one point.
fn line(cell: Rect, width: Width) -> Rect {
    let high = width.points().clamp(1.0, cell.height - 2.0 * LINE_INSET);
    Rect {
        x: cell.x + LINE_INSET,
        y: cell.y + (cell.height - high) / 2.0,
        width: cell.width - 2.0 * LINE_INSET,
        height: high,
    }
}

/// The room between a width's line and the edges of its cell, in points.
pub(super) const LINE_INSET: f64 = 4.0;
