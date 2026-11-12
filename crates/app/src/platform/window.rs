//! The window itself: its style, its smallest size, its delegate, and the
//! frame it opens at and keeps (`remember`).

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2::{MainThreadMarker, MainThreadOnly, msg_send};
use objc2_app_kit::{NSBackingStoreType, NSWindowStyleMask};
use objc2_foundation::{NSPoint, NSRect, NSSize};

use super::KeyWindow;
use super::remember::{self, Kept};
use crate::layout::{self, Size};

/// A window of `size`, centred, then put where the `kept` frame was last
/// left, which it keeps from now on.
pub(super) fn make(mtm: MainThreadMarker, size: Size, kept: Kept) -> Retained<KeyWindow> {
    let size = NSSize::new(size.width, size.height);
    let style = NSWindowStyleMask::Titled
        | NSWindowStyleMask::Closable
        | NSWindowStyleMask::Miniaturizable
        | NSWindowStyleMask::Resizable;
    let frame = NSRect::new(NSPoint::new(0.0, 0.0), size);
    // SAFETY: `NSWindow`'s designated initializer on a fresh allocation of a
    // subclass with no ivars, called once with valid arguments.
    let window: Retained<KeyWindow> = unsafe {
        msg_send![
            KeyWindow::alloc(mtm),
            initWithContentRect: frame,
            styleMask: style,
            backing: NSBackingStoreType::Buffered,
            defer: false,
        ]
    };
    // SAFETY: `Retained` owns the window, so `AppKit` must not also release it on close.
    unsafe { window.setReleasedWhenClosed(false) };
    window.setAcceptsMouseMovedEvents(true);
    window.setContentMinSize(NSSize::new(layout::SMALLEST.width, layout::SMALLEST.height));
    // The delegate is weak: the window lives as long as the app.
    window.setDelegate(Some(ProtocolObject::from_ref(&*window)));
    window.center();
    remember::frame(&window, kept);
    window
}
