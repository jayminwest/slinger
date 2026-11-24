//! SPIKE (slinger-dd9b7b): paints Nvim's cell grid ([`crate::nvim::grid`])
//! in a canvas view: each row's backgrounds, then its text in runs of one
//! highlight in a fixed-pitch font, the underlines, and the cursor in its
//! mode's shape. The grid says every colour and shape; nothing is decided
//! here. A cell is half a size wide and a size tall, which is Menlo's cell
//! at the size a canvas text is set at ([`glyphs::FONT`]), so a run of
//! plain characters keeps to its cells; any other character is drawn alone
//! in its own cell.

use std::collections::HashMap;

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{AnyThread, msg_send};
use objc2_app_kit::NSAffineTransformNSAppKitAdditions;
use objc2_app_kit::{
    NSBezierPath, NSFont, NSFontAttributeName, NSForegroundColorAttributeName, NSGraphicsContext,
    NSRectFill,
};
use objc2_foundation::{
    NSAffineTransform, NSAttributedString, NSAttributedStringKey, NSDictionary, NSPoint, NSRect,
    NSSize, NSString,
};

use super::color;
use crate::config::Rgb;
use crate::layout::Point;
use crate::nvim::grid::{Cell, CursorShape, Grid, Style, Under};
use crate::scene::glyphs;

/// The attributes a run is drawn with, kept by weight, slant and colour.
type Attributes = Retained<NSDictionary<NSAttributedStringKey, AnyObject>>;

struct Pen {
    size: f64,
    kept: HashMap<(bool, bool, [u8; 3]), Attributes>,
}

impl Pen {
    fn attributes(&mut self, style: &Style, fg: Rgb) -> &Attributes {
        let size = self.size;
        self.kept.entry((style.bold, style.italic, [fg.r, fg.g, fg.b])).or_insert_with(|| {
            let name = match (style.bold, style.italic) {
                (false, false) => "Menlo-Regular",
                (true, false) => "Menlo-Bold",
                (false, true) => "Menlo-Italic",
                (true, true) => "Menlo-BoldItalic",
            };
            let font = NSFont::fontWithName_size(&NSString::from_str(name), size)
                .or_else(|| NSFont::userFixedPitchFontOfSize(size))
                .unwrap_or_else(|| NSFont::systemFontOfSize(size));
            let ink = color(fg);
            // SAFETY: the two keys are AppKit's own statics.
            let keys: [&NSAttributedStringKey; 2] =
                unsafe { [NSFontAttributeName, NSForegroundColorAttributeName] };
            let values: [&AnyObject; 2] = [&font, &ink];
            NSDictionary::from_slices(&keys, &values)
        })
    }

    /// Draws `text` from the top left `at`.
    fn draw(&mut self, text: &str, style: &Style, fg: Rgb, at: NSPoint) {
        let attributes = self.attributes(style, fg);
        // SAFETY: each value is the type its key takes: an `NSFont` and an `NSColor`.
        let string = unsafe {
            NSAttributedString::initWithString_attributes(
                NSAttributedString::alloc(),
                &NSString::from_str(text),
                Some(attributes),
            )
        };
        // SAFETY: `drawAtPoint:` takes a point and returns nothing.
        let () = unsafe { msg_send![&*string, drawAtPoint: at] };
    }
}

fn fill(colour: Rgb, x: f64, y: f64, width: f64, height: f64) {
    color(colour).setFill();
    NSRectFill(NSRect::new(NSPoint::new(x, y), NSSize::new(width, height)));
}

/// A cell one plain character fills, which a run may hold.
fn plain(cell: &Cell) -> bool {
    matches!(cell.text.as_bytes(), [b' '..=b'~'])
}

/// Paints `grid` from `at`, its cells half a `size` wide and a `size`
/// tall, `zoom` times that.
#[expect(clippy::cast_precision_loss, reason = "a grid's cells are far below 2^52")]
pub(super) fn paint(grid: &Grid, at: Point, size: f64, zoom: f64) {
    NSGraphicsContext::saveGraphicsState_class();
    let place = NSAffineTransform::transform();
    place.translateXBy_yBy(at.x, at.y);
    place.scaleBy(zoom);
    place.concat();
    let (w, h) = (size * glyphs::CELL, size);
    let font = size * glyphs::FONT;
    // Menlo's line is 1.164 of its size: the text sits in the cell's middle.
    let drop = (h - font * 1.164) / 2.0;
    let mut pen = Pen { size: font, kept: HashMap::new() };
    fill(grid.background(), 0.0, 0.0, grid.cols as f64 * w, grid.rows as f64 * h);
    for row in 0..grid.rows {
        let (cells, y) = (grid.row(row), row as f64 * h);
        let mut col = 0;
        while col < cells.len() {
            let start = col;
            while col < cells.len() && cells[col].hl == cells[start].hl {
                col += 1;
            }
            let style = grid.style(cells[start].hl);
            let (x, width) = (start as f64 * w, (col - start) as f64 * w);
            if style.bg != grid.background() {
                fill(style.bg, x, y, width, h);
            }
            under(&style, x, y, width, h);
        }
        let mut col = 0;
        while col < cells.len() {
            let (start, cell) = (col, &cells[col]);
            col += 1;
            if cell.text.is_empty() || cell.text == " " {
                continue;
            }
            let style = grid.style(cell.hl);
            let mut run = cell.text.clone();
            if plain(cell) {
                while col < cells.len() && cells[col].hl == cell.hl && plain(&cells[col]) {
                    run.push_str(&cells[col].text);
                    col += 1;
                }
            }
            pen.draw(run.trim_end(), &style, style.fg, NSPoint::new(start as f64 * w, y + drop));
        }
    }
    cursor(grid, &mut pen, (w, h), drop);
    NSGraphicsContext::restoreGraphicsState_class();
}

/// The underline or strike of a run of cells from `x`, `width` wide.
fn under(style: &Style, x: f64, y: f64, width: f64, h: f64) {
    let thick = (h / 16.0).max(0.5);
    let line = |at: f64| {
        let path = NSBezierPath::new();
        path.moveToPoint(NSPoint::new(x, at));
        path.lineToPoint(NSPoint::new(x + width, at));
        path.setLineWidth(thick);
        path
    };
    let low = y + h - 1.5 * thick;
    if style.strike {
        color(style.fg).setStroke();
        line(y + h * 0.55).stroke();
    }
    if style.under == Under::None {
        return;
    }
    color(style.sp).setStroke();
    match style.under {
        Under::None => {}
        Under::Line => line(low).stroke(),
        Under::Double => {
            line(low).stroke();
            line(low - 2.0 * thick).stroke();
        }
        Under::Dotted | Under::Dashed => {
            let step = if style.under == Under::Dotted { thick } else { 3.0 * thick };
            let (path, dash) = (line(low), [step, step]);
            // SAFETY: the pattern is two floats that outlive the call.
            unsafe { path.setLineDash_count_phase(dash.as_ptr(), 2, 0.0) };
            path.stroke();
        }
        Under::Curl => {
            let path = NSBezierPath::new();
            let (step, mut at, mut up) = (h / 8.0, x, false);
            path.moveToPoint(NSPoint::new(x, low));
            while at < x + width {
                at = (at + step).min(x + width);
                path.lineToPoint(NSPoint::new(at, if up { low } else { low - 1.5 * thick }));
                up = !up;
            }
            path.setLineWidth(thick);
            path.stroke();
        }
    }
}

/// The cursor: a block shows its cell's text in the cell's colours swapped.
#[expect(clippy::cast_precision_loss, reason = "a grid's cells are far below 2^52")]
fn cursor(grid: &Grid, pen: &mut Pen, (w, h): (f64, f64), drop: f64) {
    let Some(shape) = grid.cursor_shape() else { return };
    let (row, col) = grid.cursor;
    let Some(cell) = grid.row(row).get(col) else { return };
    let style = grid.style(cell.hl);
    let (x, y) = (col as f64 * w, row as f64 * h);
    match shape {
        CursorShape::Block => {
            let wide = grid.row(row).get(col + 1).is_some_and(|next| next.text.is_empty());
            fill(style.fg, x, y, if wide { 2.0 * w } else { w }, h);
            if !cell.text.trim().is_empty() {
                pen.draw(&cell.text, &style, style.bg, NSPoint::new(x, y + drop));
            }
        }
        CursorShape::Vertical(part) => fill(style.fg, x, y, (w * part).max(1.0), h),
        CursorShape::Horizontal(part) => fill(style.fg, x, y + h * (1.0 - part), w, h * part),
    }
}
