//! An overlay over the window (the key overlay, the palette): a box that
//! dims the window, then per line a box for its fill and edge, with its
//! corners as round as the line says, and its text in its font, set as the
//! line says and centred top to bottom in its frame.

use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2_app_kit::{
    NSBox, NSBoxType, NSColor, NSFont, NSLineBreakMode, NSTextAlignment, NSTitlePosition, NSView,
};
use objc2_foundation::{NSPoint, NSRect, NSSize};

use super::{color, flipped, label};
use crate::scene::Label;
use crate::scene::overlay::{Align, Line, Overlay};

/// The views that draw `overlay` in a view `height` tall, bottom first.
pub(super) fn views(
    overlay: &Overlay,
    height: f64,
    mtm: MainThreadMarker,
) -> Vec<Retained<NSView>> {
    let dim = plain(mtm);
    dim.setBorderWidth(0.0);
    let fill: Retained<NSColor> = color(overlay.dim).colorWithAlphaComponent(overlay.opacity);
    dim.setFillColor(&fill);
    dim.setFrame(flipped(height, overlay.frame));
    let mut views = vec![Retained::into_super(dim)];
    for line in &overlay.lines {
        if line.label.fill.is_some() || line.edge.is_some() {
            views.push(Retained::into_super(back(line, height, mtm)));
        }
        if !line.label.text.is_empty() {
            views.push(text(line, height, mtm));
        }
    }
    views
}

/// The box behind `line`: its fill (or none), its edge one point wide (or
/// none) and its corners.
pub(super) fn back(line: &Line, height: f64, mtm: MainThreadMarker) -> Retained<NSBox> {
    let back = plain(mtm);
    back.setCornerRadius(line.radius);
    back.setFillColor(&line.label.fill.map_or_else(NSColor::clearColor, color));
    match line.edge {
        Some(edge) => {
            back.setBorderWidth(1.0);
            back.setBorderColor(&color(edge));
        }
        None => back.setBorderWidth(0.0),
    }
    back.setFrame(flipped(height, line.label.frame));
    back
}

/// `line`'s text in its font, as tall as the font needs and centred top
/// to bottom in the line's frame, so a row's words sit in its middle.
fn text(line: &Line, height: f64, mtm: MainThreadMarker) -> Retained<NSView> {
    let align = match line.align {
        Align::Left => NSTextAlignment::Left,
        Align::Centre => NSTextAlignment::Center,
        Align::Right => NSTextAlignment::Right,
    };
    let bare = Label { fill: None, ..line.label.clone() };
    let field = label(&bare, height, align, mtm);
    let font = if line.bold {
        NSFont::boldSystemFontOfSize(line.size)
    } else {
        NSFont::systemFontOfSize(line.size)
    };
    field.setFont(Some(&font));
    field.setLineBreakMode(NSLineBreakMode::ByTruncatingTail);
    let frame = flipped(height, line.label.frame);
    let tall = field.cell().map_or(frame.size.height, |cell| cell.cellSize().height);
    let tall = tall.min(frame.size.height);
    let y = frame.origin.y + (frame.size.height - tall) / 2.0;
    field.setFrame(NSRect::new(
        NSPoint::new(frame.origin.x, y),
        NSSize::new(frame.size.width, tall),
    ));
    Retained::into_super(Retained::into_super(field))
}

/// A box with no title that draws as its colours say.
fn plain(mtm: MainThreadMarker) -> Retained<NSBox> {
    let block = NSBox::new(mtm);
    block.setBoxType(NSBoxType::Custom);
    block.setTitlePosition(NSTitlePosition::NoTitle);
    block.setContentViewMargins(NSSize::new(0.0, 0.0));
    block
}
