//! Styled text: a sidebar row in its font, and a pane's footer (its fill,
//! the pen's swatch, and each part's runs one after the other in one
//! field). The scene decides every word, colour and frame.

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{AnyThread, MainThreadMarker};
use objc2_app_kit::{
    NSFont, NSFontAttributeName, NSForegroundColorAttributeName, NSKernAttributeName,
    NSLineBreakMode, NSTextAlignment, NSTextField, NSView,
};
use objc2_foundation::{
    NSAttributedString, NSAttributedStringKey, NSDictionary, NSMutableAttributedString, NSNumber,
    NSString,
};

use super::{color, label};
use crate::config::Rgb;
use crate::scene::Label;
use crate::scene::sidebar::Row;
use crate::scene::status::{self, Align, Footer};

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

/// The views that show `footer` in a view `height` tall: its fill, the
/// swatch, then one field per part.
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
    views.extend(footer.swatch.map(|(frame, colour)| filled(frame, colour)));
    for part in &footer.parts {
        let align = match part.align {
            Align::Left => NSTextAlignment::Left,
            Align::Centre => NSTextAlignment::Center,
            Align::Right => NSTextAlignment::Right,
        };
        let colour = part.runs.first().map_or(Rgb { r: 0, g: 0, b: 0 }, |r| r.colour);
        let blank = Label { text: String::new(), colour, fill: None, frame: part.frame };
        let field = label(&blank, height, align, mtm);
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
        // The alignment is the field's: set after its text, which would reset it.
        field.setAlignment(align);
        field.setLineBreakMode(NSLineBreakMode::ByTruncatingTail);
        views.push(Retained::into_super(Retained::into_super(field)));
    }
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
