//! Text: a label (its fill over its whole frame, its words centred in it
//! both ways), a sidebar row in its font, and a pane's footer (its fill,
//! the lit button's rounded fill, its marks, each part's runs one after
//! the other in one field, and the lit button's tip). The scene decides every word, colour and frame.

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{AnyThread, MainThreadMarker};
use objc2_app_kit::{
    NSFont, NSFontAttributeName, NSForegroundColorAttributeName, NSKernAttributeName,
    NSLineBreakMode, NSTextAlignment, NSTextField, NSView,
};
use objc2_foundation::{
    NSAttributedString, NSAttributedStringKey, NSDictionary, NSMutableAttributedString, NSNumber,
    NSPoint, NSRect, NSSize, NSString,
};

use super::{color, flipped, label};
use crate::config::Rgb;
use crate::scene::Label;
use crate::scene::overlay::Line;
use crate::scene::sidebar::Row;
use crate::scene::status::{self, Footer};

/// The views that show `line` in a view `height` tall: its fill over its
/// whole frame, then its words in the middle of it, side to side and top
/// to bottom, so a pill or a button taller than its text holds it centred.
pub(super) fn line(line: &Label, height: f64, mtm: MainThreadMarker) -> Vec<Retained<NSView>> {
    let into = |field: Retained<NSTextField>| Retained::into_super(Retained::into_super(field));
    if line.text.is_empty() {
        return vec![into(label(line, height, NSTextAlignment::Center, mtm))];
    }
    let mut views = Vec::new();
    if let Some(fill) = line.fill {
        let block =
            Label { text: String::new(), colour: fill, fill: Some(fill), frame: line.frame };
        views.push(into(label(&block, height, NSTextAlignment::Center, mtm)));
    }
    let bare = Label { fill: None, ..line.clone() };
    let field = label(&bare, height, NSTextAlignment::Center, mtm);
    // A line longer than its frame keeps both ends: paths end in the part that matters.
    field.setLineBreakMode(NSLineBreakMode::ByTruncatingMiddle);
    centre(&field, flipped(height, line.frame));
    views.push(into(field));
    views
}

/// Sets `field` as tall as its text in its font (at most `frame`), in the
/// middle of `frame` top to bottom and as wide as it.
pub(super) fn centre(field: &NSTextField, frame: NSRect) {
    let tall = field.cell().map_or(frame.size.height, |cell| cell.cellSize().height);
    let tall = tall.min(frame.size.height);
    let y = frame.origin.y + (frame.size.height - tall) / 2.0;
    field.setFrame(NSRect::new(
        NSPoint::new(frame.origin.x, y),
        NSSize::new(frame.size.width, tall),
    ));
}

/// A text field that shows a sidebar `row`, read from its left edge, in
/// its font, in a view `height` tall.
pub(super) fn sidebar_row(row: &Row, height: f64, mtm: MainThreadMarker) -> Retained<NSTextField> {
    let field = label(&row.label, height, NSTextAlignment::Left, mtm);
    let size = row.font.size.unwrap_or_else(NSFont::systemFontSize);
    let (text, colour) = (&row.label.text, row.label.colour);
    field.setAttributedStringValue(&styled(text, colour, size, row.font.bold, row.font.spacing));
    field.setLineBreakMode(NSLineBreakMode::ByTruncatingTail);
    field
}

/// The views that show `footer` in a view `height` tall: its fill, its
/// top hairline, the lit button's fill, the marks, one field per part,
/// then the lit button's tip above it.
pub(super) fn footer(footer: &Footer, height: f64, mtm: MainThreadMarker) -> Vec<Retained<NSView>> {
    let filled = |frame, fill| {
        let block = Label { text: String::new(), colour: fill, fill: Some(fill), frame };
        Retained::into_super(Retained::into_super(label(
            &block,
            height,
            NSTextAlignment::Left,
            mtm,
        )))
    };
    let mut views = vec![filled(footer.frame, footer.fill)];
    views.push(filled(footer.rule.frame, footer.rule.colour));
    if let Some((frame, fill)) = footer.lit {
        let lit = Line::fill(fill, frame, status::LIT_RADIUS);
        views.push(Retained::into_super(super::overlay::back(&lit, height, mtm)));
    }
    views.extend(footer.marks.iter().map(|&(frame, colour)| filled(frame, colour)));
    for part in &footer.parts {
        let colour = part.runs.first().map_or(Rgb { r: 0, g: 0, b: 0 }, |r| r.colour);
        let blank = Label { text: String::new(), colour, fill: None, frame: part.frame };
        let field = label(&blank, height, NSTextAlignment::Left, mtm);
        let text = NSMutableAttributedString::new();
        for run in &part.runs {
            text.appendAttributedString(&styled(
                &run.text,
                run.colour,
                status::TEXT_SIZE,
                run.bold,
                0.0,
            ));
        }
        field.setAttributedStringValue(&text);
        field.setLineBreakMode(NSLineBreakMode::ByTruncatingTail);
        // A field does not align styled text, so the field itself is placed:
        // as wide as its text (at most the part), in the part's middle.
        field.sizeToFit();
        let mut frame = flipped(height, part.frame);
        let width = field.frame().size.width.min(frame.size.width);
        frame.origin.x += (frame.size.width - width) / 2.0;
        frame.size.width = width;
        field.setFrame(frame);
        views.push(Retained::into_super(Retained::into_super(field)));
    }
    views.extend(footer.tip.iter().flat_map(|tip| super::overlay::line(tip, height, mtm)));
    views
}

/// `text` in `colour`, the system font `size` points high (bold or not),
/// with `spacing` points added between its letters.
fn styled(
    text: &str,
    colour: Rgb,
    size: f64,
    bold: bool,
    spacing: f64,
) -> Retained<NSAttributedString> {
    let font =
        if bold { NSFont::boldSystemFontOfSize(size) } else { NSFont::systemFontOfSize(size) };
    let (ink, kern) = (color(colour), NSNumber::new_f64(spacing));
    // SAFETY: the three keys are AppKit's own statics.
    let keys: [&NSAttributedStringKey; 3] =
        unsafe { [NSFontAttributeName, NSForegroundColorAttributeName, NSKernAttributeName] };
    let values: [&AnyObject; 3] = [&font, &ink, &kern];
    let attributes = NSDictionary::from_slices(&keys, &values);
    // SAFETY: each value is the type its key takes: an `NSFont`, an `NSColor`
    // and an `NSNumber`.
    unsafe {
        NSAttributedString::initWithString_attributes(
            NSAttributedString::alloc(),
            &NSString::from_str(text),
            Some(&attributes),
        )
    }
}
