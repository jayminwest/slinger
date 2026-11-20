//! The window's input: a key press becomes a [`Chord`] (or the text it
//! types), a left-button or pen event a [`Pen`], a two-finger scroll or a
//! wheel an [`Event::Scroll`] and a pinch an [`Event::Zoom`], each handed
//! to the app. Makes no decisions, only translations:
//!
//! - A scroll (a wheel, two fingers) with command or control held is an
//!   [`Event::Zoom`] about the pointer, as a pinch there would be: a mouse
//!   has no pinch. A scroll away from the user (as the hand moved, natural
//!   scrolling or not) zooms in ([`scroll_zoom`]).
//! - A right- or middle-button press, drag or lift is an [`Event::Button`],
//!   raw: the app decides a pan from a context click. A left press with
//!   control held is the secondary button's, and so are the drags and the
//!   lift after it, as macOS defines ctrl+click; that is the one state kept.
//! - A tablet pen's end is known from its proximity events (the pen came
//!   near the tablet, or left): each says [`Event::Proximity`], and each
//!   pen event after it carries it ([`Pen::eraser`]). `AppKit` names the
//!   end only on those events, so it is kept here between them. A pen
//!   event is a tablet's ([`Pen::tablet`]) by its subtype.
//! - Space with no modifier says [`Event::Space`] before it goes on as a
//!   key, and stops there if taken; its key up says it too.
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
use crate::commands::Chord;
use crate::layout::{Point, scroll_zoom};
use crate::scene::cursor::Cursor;
use crate::scene::ink::Pen;
use crate::scene::secondary::{Button, Phase};

/// Space with no modifier: held, a drag pans ([`Event::Space`]).
const SPACE: Chord = Chord::new(0, ' ');
/// How far one line of a mouse wheel scrolls, in points. A trackpad gives points.
const WHEEL_LINE: f64 = 16.0;
/// A scroll with either of these held zooms.
const ZOOMS: NSEventModifierFlags =
    NSEventModifierFlags::Command.union(NSEventModifierFlags::Control);

thread_local! {
    /// Whether the tablet pen near the tablet has its eraser end down.
    static ERASER: Cell<bool> = const { Cell::new(false) };
    /// Whether the left press under way went down with control held: it,
    /// its drags and its lift are the secondary button's.
    static CONTROL: Cell<bool> = const { Cell::new(false) };
    /// The scene's cursor over a divider between panes, if any.
    pub(super) static DIVIDER: Cell<Option<Cursor>> = const { Cell::new(None) };
}

/// Hands `event`, sent to `window`, to the app as an [`Event`]. True when
/// the app used it, so it goes no further; moves, proximity and key ups
/// always go on.
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
        dispatch(Event::Proximity { near: event.isEnteringProximity(), eraser }, mtm);
    }
    let middle = || event.buttonNumber() == 2;
    match kind {
        NSEventType::KeyDown
            if chord(event) == Some(SPACE) && dispatch(Event::Space(true), mtm) =>
        {
            true
        }
        NSEventType::KeyUp if chord(event).is_some_and(|c| c.key() == ' ') => {
            dispatch(Event::Space(false), mtm);
            false
        }
        NSEventType::KeyDown => {
            chord(event).is_some_and(|chord| dispatch(Event::Key(chord), mtm))
                || typed(event).is_some_and(|text| dispatch(Event::Text(text), mtm))
        }
        // Each left press says afresh whether control makes it secondary:
        // a lift `AppKit` swallowed leaves nothing behind.
        NSEventType::LeftMouseDown => {
            let control = event.modifierFlags().contains(NSEventModifierFlags::Control);
            CONTROL.set(control);
            let event = if control {
                button(Button::Secondary, Phase::Down, at())
            } else {
                Event::Press(pen(window, event))
            };
            dispatch(event, mtm)
        }
        NSEventType::LeftMouseDragged if CONTROL.get() => {
            dispatch(button(Button::Secondary, Phase::Drag, at()), mtm)
        }
        NSEventType::LeftMouseUp if CONTROL.replace(false) => {
            dispatch(button(Button::Secondary, Phase::Up, at()), mtm)
        }
        NSEventType::LeftMouseDragged => dispatch(Event::Drag(pen(window, event)), mtm),
        NSEventType::LeftMouseUp => dispatch(Event::Lift(pen(window, event)), mtm),
        NSEventType::OtherMouseDown
        | NSEventType::OtherMouseDragged
        | NSEventType::OtherMouseUp
            if !middle() =>
        {
            false
        }
        NSEventType::RightMouseDown | NSEventType::OtherMouseDown => {
            dispatch(button(which(kind), Phase::Down, at()), mtm)
        }
        NSEventType::RightMouseDragged | NSEventType::OtherMouseDragged => {
            dispatch(button(which(kind), Phase::Drag, at()), mtm)
        }
        NSEventType::RightMouseUp | NSEventType::OtherMouseUp => {
            dispatch(button(which(kind), Phase::Up, at()), mtm)
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

/// A right or middle button's event.
fn button(button: Button, phase: Phase, at: Point) -> Event {
    Event::Button { button, phase, at }
}

/// The button of a right- or middle-button event of `kind`.
fn which(kind: NSEventType) -> Button {
    match kind {
        NSEventType::RightMouseDown
        | NSEventType::RightMouseDragged
        | NSEventType::RightMouseUp => Button::Secondary,
        _ => Button::Middle,
    }
}

/// Where a pen or mouse event is in `window`'s content, from the top left.
fn point(window: &NSWindow, event: &NSEvent) -> Point {
    let at = event.locationInWindow();
    let height = window.contentView().map_or(0.0, |v| v.frame().size.height);
    Point { x: at.x, y: height - at.y }
}

/// A left-button or pen event: where, when, whether a finger made it,
/// whether a tablet pen's eraser end did, whether shift was held, and
/// whether a tablet pen made it at all.
fn pen(window: &NSWindow, event: &NSEvent) -> Pen {
    let subtype = event.subtype();
    let touch = subtype == NSEventSubtype::Touch;
    let tablet = matches!(subtype, NSEventSubtype::TabletPoint | NSEventSubtype::TabletProximity);
    let eraser = tablet && ERASER.get();
    let shift = event.modifierFlags().contains(NSEventModifierFlags::Shift);
    Pen { at: point(window, event), when: event.timestamp(), touch, eraser, shift, tablet }
}
