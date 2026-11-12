//! The chrome that is not a line of text: the focus border, and the flash's
//! pill with the one timer that takes it away; and what every view is made
//! with: a plain label, a frame in `AppKit`'s coordinates and a colour.

use std::time::Instant;

use objc2::rc::Retained;
use objc2::{MainThreadMarker, sel};
use objc2_app_kit::{
    NSBox, NSBoxType, NSColor, NSFont, NSTextAlignment, NSTextField, NSTitlePosition, NSWindow,
};
use objc2_foundation::{NSPoint, NSRect, NSSize, NSString, NSTimer};

use crate::config::Rgb;
use crate::layout::Rect;
use crate::scene::flash::{self, Pill};
use crate::scene::{Border, Label};

/// A box that draws only its edge: no title, no fill.
pub fn border(border: &Border, height: f64, mtm: MainThreadMarker) -> Retained<NSBox> {
    let edge = frameless(mtm);
    edge.setBorderWidth(border.width);
    edge.setBorderColor(&color(border.colour));
    edge.setFillColor(&NSColor::clearColor());
    edge.setFrame(flipped(height, border.frame));
    edge
}

/// The flash `pill` in a view `height` tall: its text centred in a rounded
/// box of its fill. The window ticks once when it is due to go, so the app
/// draws the window again without it.
pub fn pill(window: &NSWindow, pill: &Pill, height: f64, mtm: MainThreadMarker) -> Retained<NSBox> {
    let line = &pill.label;
    let back = frameless(mtm);
    back.setBorderWidth(0.0);
    back.setCornerRadius(flash::CORNER);
    back.setFillColor(&color(line.fill.unwrap_or(line.colour)));
    back.setContentViewMargins(NSSize::new(0.0, 0.0));
    back.setFrame(flipped(height, line.frame));
    let text = NSTextField::labelWithString(&NSString::from_str(&line.text), mtm);
    text.setFont(Some(&NSFont::systemFontOfSize(flash::TEXT)));
    text.setTextColor(Some(&color(line.colour)));
    text.setAlignment(NSTextAlignment::Center);
    text.sizeToFit();
    let tall = text.frame().size.height;
    let (width, room) = (line.frame.width, line.frame.height);
    text.setFrame(NSRect::new(NSPoint::new(0.0, (room - tall) / 2.0), NSSize::new(width, tall)));
    back.addSubview(&text);
    let due = pill.until.saturating_duration_since(Instant::now()).as_secs_f64();
    // SAFETY: the window implements `tick:`, which takes the timer. The run
    // loop keeps the timer until it fires once, and the timer keeps its target.
    let _timer = unsafe {
        NSTimer::scheduledTimerWithTimeInterval_target_selector_userInfo_repeats(
            due,
            window,
            sel!(tick:),
            None,
            false,
        )
    };
    back
}

/// A box with no title that draws as its colours say.
fn frameless(mtm: MainThreadMarker) -> Retained<NSBox> {
    let edge = NSBox::new(mtm);
    edge.setBoxType(NSBoxType::Custom);
    edge.setTitlePosition(NSTitlePosition::NoTitle);
    edge
}

/// A text field that shows `line` in a view `height` tall.
pub fn label(
    line: &Label,
    height: f64,
    align: NSTextAlignment,
    mtm: MainThreadMarker,
) -> Retained<NSTextField> {
    let field = NSTextField::labelWithString(&NSString::from_str(&line.text), mtm);
    field.setFrame(flipped(height, line.frame));
    field.setAlignment(align);
    field.setTextColor(Some(&color(line.colour)));
    if let Some(fill) = line.fill {
        field.setDrawsBackground(true);
        field.setBackgroundColor(Some(&color(fill)));
    }
    field
}

/// `rect`, whose origin is the top left of a view `height` tall, in `AppKit`'s
/// coordinates, whose origin is the bottom left.
pub fn flipped(height: f64, rect: Rect) -> NSRect {
    NSRect::new(
        NSPoint::new(rect.x, height - rect.y - rect.height),
        NSSize::new(rect.width, rect.height),
    )
}

pub fn color(rgb: Rgb) -> Retained<NSColor> {
    let channel = |c: u8| f64::from(c) / 255.0;
    NSColor::colorWithSRGBRed_green_blue_alpha(channel(rgb.r), channel(rgb.g), channel(rgb.b), 1.0)
}
