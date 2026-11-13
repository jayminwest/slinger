//! The window's input: a key press becomes a [`Chord`] (or the text it
//! types), a left-button or pen event a [`Pen`], a two-finger scroll or a
//! wheel an [`Event::Scroll`] and a pinch an [`Event::Zoom`], each handed
//! to the app. Makes no decisions, only translations:
//!
//! - A scroll (a wheel, two fingers) with command or control held is an
//!   [`Event::Zoom`] about the pointer, as a pinch there would be: a mouse
//!   has no pinch. A scroll away from the user (as the hand moved, natural
//!   scrolling or not) zooms in ([`scroll_zoom`]).
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
//!   Where the scene says the pointer is on a divider between panes, or
//!   holds one, it is the divider's cursor over whatever view is there.
//! - A press that went on to the view under it (an agent UI's page takes
//!   the keyboard) gives an open entry the keyboard back ([`went_on`]).

use std::cell::Cell;

use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2_app_kit::{
    NSCursor, NSEvent, NSEventModifierFlags, NSEventSubtype, NSEventType, NSPointingDeviceType,
    NSView, NSWindow,
};
use objc2_foundation::NSPoint;

use super::keys::{chord, typed};
use super::{CanvasView, Event, STATE, dispatch};
use crate::layout::{Point, scroll_zoom};
use crate::scene::cursor::Cursor;
use crate::scene::ink::Pen;

/// How far one line of a mouse wheel scrolls, in points. A trackpad gives points.
const WHEEL_LINE: f64 = 16.0;
/// A scroll with either of these held zooms.
const ZOOMS: NSEventModifierFlags =
    NSEventModifierFlags::Command.union(NSEventModifierFlags::Control);

thread_local! {
    /// Whether the tablet pen near the tablet has its eraser end down.
    static ERASER: Cell<bool> = const { Cell::new(false) };
    /// The right-button drag under way, if any: where it was pressed and
    /// where it was last, in the window from its top left.
    static RIGHT: Cell<Option<(Point, Point)>> = const { Cell::new(None) };
    /// The scene's cursor over a divider between panes, if any.
    pub(super) static DIVIDER: Cell<Option<Cursor>> = const { Cell::new(None) };
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
            if event.modifierFlags().intersects(ZOOMS) {
                // As the hand moved: natural scrolling turns the deltas round.
                let away = if event.isDirectionInvertedFromDevice() { -dy } else { dy };
                return dispatch(Event::Zoom { at: at(), factor: scroll_zoom(away) }, mtm);
            }
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

/// `event` went on to the view under it: after a move the pointer gets the
/// cursor of what is under it; after a press (on an agent UI's page, which
/// took the keyboard) an open entry takes the keyboard back at once
/// ([`crate::scene::paste::Keyboard::Entry`]).
pub(super) fn went_on(window: &NSWindow, event: &NSEvent) {
    match event.r#type() {
        NSEventType::MouseMoved => cursor(window, event.locationInWindow()),
        NSEventType::LeftMouseDown => {
            let shown = STATE.get().and_then(|state| state.shown.try_borrow().ok());
            if let Some(entry) = shown.as_ref().and_then(|shown| shown.entry.as_ref()) {
                entry.take_back(window);
            }
        }
        _ => {}
    }
}

/// Sets the cursor for the pointer at `at`, in `window`'s base coordinates:
/// on a divider the scene's cursor for it ([`divider`]); over a canvas view its cursor, also where chrome that takes no press (the
/// focus border, a label, the flash) lies over the canvas; over the rest of
/// the chrome, the bare window and all of it under the key overlay or the
/// palette, the arrow. Over anything else (an agent UI, the sling's entry)
/// that view sets its own. A box is hit as its content view, so the chrome
/// is told by descent, not by being the view hit.
pub(super) fn cursor(window: &NSWindow, at: NSPoint) {
    if divider() {
        return;
    }
    let Some(state) = STATE.get() else { return };
    let Some(content) = window.contentView() else { return };
    // The content view's superview has the window's base coordinates.
    let Some(hit) = content.hitTest(at) else { return };
    let Ok(shown) = state.shown.try_borrow() else { return };
    let is = |view: &NSView| std::ptr::eq(view, Retained::as_ptr(&hit));
    let chrome = is(&content) || shown.chrome.iter().any(|view| hit.isDescendantOf(view));
    let covered = shown.made_from.as_ref().is_some_and(|made| made.5.is_some());
    let inside = content.convertPoint_fromView(at, None);
    let holds = |canvas: &CanvasView| {
        let frame = canvas.frame();
        let (x, y) = (inside.x - frame.origin.x, inside.y - frame.origin.y);
        (0.0..frame.size.width).contains(&x) && (0.0..frame.size.height).contains(&y)
    };
    let under = |canvas: &CanvasView| is(canvas) || (chrome && !covered && holds(canvas));
    if let Some((_, canvas)) = shown.canvases.iter().find(|(_, canvas)| under(canvas)) {
        canvas.set_cursor();
    } else if chrome {
        NSCursor::arrowCursor().set();
    }
}

/// Sets the scene's cursor over a divider between panes, while it has one.
/// Returns whether it had.
pub(super) fn divider() -> bool {
    DIVIDER.get().map(super::canvas::ns_cursor).inspect(|cursor| cursor.set()).is_some()
}

/// The window became key: its canvases' cursor rects are made again, as a
/// tool change does, and the pointer gets its cursor now, not on its next
/// move; an open entry has the keyboard again.
pub(super) fn became_key(window: &NSWindow) {
    if let Some(shown) = STATE.get().and_then(|state| state.shown.try_borrow().ok()) {
        for (_, canvas) in &shown.canvases {
            window.invalidateCursorRectsForView(canvas);
        }
        if let Some(entry) = &shown.entry {
            entry.focus(window);
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

/// A left-button or pen event: where, when, whether a finger made it,
/// whether a tablet pen's eraser end did, and whether shift was held.
fn pen(window: &NSWindow, event: &NSEvent) -> Pen {
    let subtype = event.subtype();
    let touch = subtype == NSEventSubtype::Touch;
    let tablet = matches!(subtype, NSEventSubtype::TabletPoint | NSEventSubtype::TabletProximity);
    let eraser = tablet && ERASER.get();
    let shift = event.modifierFlags().contains(NSEventModifierFlags::Shift);
    Pen { at: point(window, event), when: event.timestamp(), touch, eraser, shift }
}
