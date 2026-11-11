//! An overlay over the window (the key overlay): a box that dims the
//! window, then a text field per line in its font, set as the line says.

use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2_app_kit::{
    NSBox, NSBoxType, NSColor, NSFont, NSLineBreakMode, NSTextAlignment, NSTitlePosition, NSView,
};

use super::{color, flipped, label};
use crate::scene::overlay::{Align, Overlay};

/// The views that draw `overlay` in a view `height` tall, bottom first.
pub(super) fn views(
    overlay: &Overlay,
    height: f64,
    mtm: MainThreadMarker,
) -> Vec<Retained<NSView>> {
    let dim = NSBox::new(mtm);
    dim.setBoxType(NSBoxType::Custom);
    dim.setTitlePosition(NSTitlePosition::NoTitle);
    dim.setBorderWidth(0.0);
    let fill: Retained<NSColor> = color(overlay.dim).colorWithAlphaComponent(overlay.opacity);
    dim.setFillColor(&fill);
    dim.setFrame(flipped(height, overlay.frame));
    let mut views = vec![Retained::into_super(dim)];
    for line in &overlay.lines {
        let align = match line.align {
            Align::Left => NSTextAlignment::Left,
            Align::Centre => NSTextAlignment::Center,
            Align::Right => NSTextAlignment::Right,
        };
        let field = label(&line.label, height, align, mtm);
        let font = if line.bold {
            NSFont::boldSystemFontOfSize(line.size)
        } else {
            NSFont::systemFontOfSize(line.size)
        };
        field.setFont(Some(&font));
        field.setLineBreakMode(NSLineBreakMode::ByTruncatingTail);
        views.push(Retained::into_super(Retained::into_super(field)));
    }
    views
}
