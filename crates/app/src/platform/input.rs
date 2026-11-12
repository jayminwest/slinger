//! The window's input: a key press becomes a [`Chord`] (or the text it
//! types), a left-button or pen event a [`Pen`], a two-finger scroll or a
//! wheel an [`Event::Scroll`] and a pinch an [`Event::Zoom`], each handed
//! to the app. Makes no decisions, only translations:
//!
//! - A right-button drag is an [`Event::Scroll`] by how far the pointer
//!   moved, at the point it was pressed, so it pans what a two-finger
//!   scroll there would, and keeps to that pane.
//! - A tablet pen's end is known from its proximity events (the pen came
//!   near the tablet, or left): each says [`Event::EraserEnd`], and each
//!   pen event after it carries it ([`Pen::eraser`]). `AppKit` names the
//!   end only on those events, so it is kept here between them.
//! - The pointer's cursor is set again from the view under it on each move
//!   and when the window becomes key: a canvas view's cursor over a canvas,
//!   the arrow over the chrome. `AppKit`'s cursor rects alone leave a stale
//!   cursor after the window was in the background, or past a canvas's edge.

use std::cell::Cell;

use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2_app_kit::{
    NSCursor, NSEvent, NSEventSubtype, NSEventType, NSPointingDeviceType, NSView, NSWindow,
};
use objc2_foundation::NSPoint;

use super::keys::{chord, typed};
use super::{Event, STATE, dispatch};
use crate::layout::Point;
use crate::scene::ink::Pen;

/// How far one line of a mouse wheel scrolls, in points. A trackpad gives points.
const WHEEL_LINE: f64 = 16.0;

thread_local! {
    /// Whether the tablet pen near the tablet has its eraser end down.
    static ERASER: Cell<bool> = const { Cell::new(false) };
    /// The right-button drag under way, if any: where it was pressed and
    /// where it was last, in the window from its top left.
    static RIGHT: Cell<Option<(Point, Point)>> = const { Cell::new(None) };
}

/// Hands `event`, sent to `window`, to the app as an [`Event`]. True when
/// the app used it, so it goes no further; moves, proximity and right-button
/// presses always go on.
pub(super) fn send(window: &NSWindow, event: &NSEvent, mtm: MainThreadMarker) -> bool {
    let kind = event.r#type();
    let at = || point(window, event);
    let mouse = matches!(
        kind,
        NSEventType::LeftMouseDown
            | NSEventType::LeftMouseDragged
            | NSEventType::LeftMouseUp
            | NSEventType::MouseMoved
    );
    // `AppKit` raises if a subtype or a device type is asked of any other event.
    if kind == NSEventType::TabletProximity
        || (mouse && event.subtype() == NSEventSubtype::TabletProximity)
    {
        let eraser = event.isEnteringProximity()
            && event.pointingDeviceType() == NSPointingDeviceType::Eraser;
        ERASER.set(eraser);
        dispatch(Event::EraserEnd(eraser), mtm);
    }
    match kind {
        NSEventType::KeyDown => {
            chord(event).is_some_and(|chord| dispatch(Event::Key(chord), mtm))
                || typed(event).is_some_and(|text| dispatch(Event::Text(text), mtm))
        }
        NSEventType::LeftMouseDown => dispatch(Event::Press(pen(window, event)), mtm),
        NSEventType::LeftMouseDragged => dispatch(Event::Drag(pen(window, event)), mtm),
        NSEventType::LeftMouseUp => dispatch(Event::Lift(pen(window, event)), mtm),
        NSEventType::RightMouseDown => {
            RIGHT.set(Some((at(), at())));
            false
        }
        NSEventType::RightMouseDragged => {
            let Some((from, last)) = RIGHT.get() else { return false };
            let now = at();
            RIGHT.set(Some((from, now)));
            dispatch(Event::Scroll { at: from, dx: now.x - last.x, dy: now.y - last.y }, mtm)
        }
        NSEventType::RightMouseUp => {
            RIGHT.set(None);
            false
        }
        NSEventType::ScrollWheel => {
            let unit = if event.hasPreciseScrollingDeltas() { 1.0 } else { WHEEL_LINE };
            let (dx, dy) = (event.scrollingDeltaX() * unit, event.scrollingDeltaY() * unit);
            dispatch(Event::Scroll { at: at(), dx, dy }, mtm)
        }
        NSEventType::Magnify => {
            let factor = 1.0 + event.magnification();
            dispatch(Event::Zoom { at: at(), factor }, mtm)
        }
        NSEventType::MouseMoved => {
            dispatch(Event::Moved(at()), mtm);
            false
        }
        _ => false,
    }
}

/// Sets the cursor for the pointer at `at`, in `window`'s base coordinates:
/// over a canvas view its cursor, over the chrome or the bare window the
/// arrow. Over anything else (an agent UI, the sling's entry) that view
/// sets its own.
pub(super) fn cursor(window: &NSWindow, at: NSPoint) {
    let Some(state) = STATE.get() else { return };
    let Some(content) = window.contentView() else { return };
    // The content view's superview has the window's base coordinates.
    let Some(hit) = content.hitTest(at) else { return };
    let Ok(shown) = state.shown.try_borrow() else { return };
    let is = |view: &NSView| std::ptr::eq(view, Retained::as_ptr(&hit));
    if let Some((_, canvas)) = shown.canvases.iter().find(|(_, canvas)| is(canvas)) {
        canvas.set_cursor();
    } else if is(&content) || shown.chrome.iter().any(|view| is(view)) {
        NSCursor::arrowCursor().set();
    }
}

/// The window became key: its canvases' cursor rects are made again, as a
/// tool change does, and the pointer gets its cursor now, not on its next move.
pub(super) fn became_key(window: &NSWindow) {
    if let Some(shown) = STATE.get().and_then(|state| state.shown.try_borrow().ok()) {
        for (_, canvas) in &shown.canvases {
            window.invalidateCursorRectsForView(canvas);
        }
    }
    cursor(window, window.mouseLocationOutsideOfEventStream());
}

/// Where a pen or mouse event is in `window`'s content, from the top left.
fn point(window: &NSWindow, event: &NSEvent) -> Point {
    let at = event.locationInWindow();
    let height = window.contentView().map_or(0.0, |v| v.frame().size.height);
    Point { x: at.x, y: height - at.y }
}

/// A left-button or pen event: where, when, whether a finger made it, and
/// whether a tablet pen's eraser end did.
fn pen(window: &NSWindow, event: &NSEvent) -> Pen {
    let subtype = event.subtype();
    let touch = subtype == NSEventSubtype::Touch;
    let tablet = matches!(subtype, NSEventSubtype::TabletPoint | NSEventSubtype::TabletProximity);
    let eraser = tablet && ERASER.get();
    Pen { at: point(window, event), when: event.timestamp(), touch, eraser }
}
