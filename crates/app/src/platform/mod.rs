//! The macOS layer. Every `unsafe` block and every objc2 or `AppKit` call in the
//! project is in this directory (gate check:layers, rule `platform-owns-ffi`).
//! It exposes safe functions and makes no decisions: it draws what a
//! [`Scene`] says.
//!
//! Threading rule: `AppKit` runs on the main thread only. Each function here
//! that touches `AppKit` takes a `MainThreadMarker`, which only the main thread
//! can make, so the compiler holds the rule. Blocking work (files, sockets)
//! belongs on another thread; hand its result to the main thread to draw.
#![expect(unsafe_code, reason = "the FFI boundary: AppKit is reached through objc2")]

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2::{MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSApplicationDelegate, NSBackingStoreType,
    NSColor, NSLineBreakMode, NSMenu, NSMenuItem, NSTextAlignment, NSTextField, NSWindow,
    NSWindowStyleMask,
};
use objc2_foundation::{NSObject, NSObjectProtocol, NSPoint, NSRect, NSSize, NSString};

use crate::layout::Rect;
use crate::scene::{Scene, Tone};

/// [`run`] was called off the main thread.
#[derive(Debug)]
pub struct NotMainThread;

impl std::fmt::Display for NotMainThread {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("the app must start on the main thread")
    }
}

define_class!(
    // SAFETY: `NSObject` has no subclassing requirements, and `Delegate` has
    // no `Drop` impl.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "AppDelegate"]
    struct Delegate;

    // SAFETY: the protocol's methods are optional; this class implements none of them.
    unsafe impl NSObjectProtocol for Delegate {}

    // SAFETY: the method matches the protocol's signature; the rest are optional.
    unsafe impl NSApplicationDelegate for Delegate {
        #[unsafe(method(applicationShouldTerminateAfterLastWindowClosed:))]
        fn quits_with_its_window(&self, _app: &NSApplication) -> bool {
            true
        }
    }
);

/// Shows `scene` in a window and runs the app until it quits (the window
/// closes, or Quit from the menu). Call it from `main`.
///
/// # Errors
///
/// [`NotMainThread`] when called from any other thread.
pub fn run(scene: &Scene) -> Result<(), NotMainThread> {
    let mtm = MainThreadMarker::new().ok_or(NotMainThread)?;
    let app = NSApplication::sharedApplication(mtm);
    app.setActivationPolicy(NSApplicationActivationPolicy::Regular);
    // SAFETY: `init` on a fresh allocation of a class with no ivars.
    let delegate: Retained<Delegate> = unsafe { msg_send![Delegate::alloc(mtm), init] };
    app.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
    app.setMainMenu(Some(&menu(mtm, &scene.title)));
    let window = window(mtm, scene);
    window.makeKeyAndOrderFront(None);
    app.activate();
    app.run();
    Ok(())
}

/// The menu bar: the app menu with Quit (⌘Q).
fn menu(mtm: MainThreadMarker, title: &str) -> Retained<NSMenu> {
    let quit = NSString::from_str(&format!("Quit {title}"));
    let app_menu = NSMenu::new(mtm);
    // SAFETY: `terminate:` is `NSApplication`'s action; a nil target sends it
    // up the responder chain to the app.
    unsafe {
        app_menu.addItemWithTitle_action_keyEquivalent(
            &quit,
            Some(sel!(terminate:)),
            &NSString::from_str("q"),
        );
    }
    let app_item = NSMenuItem::new(mtm);
    app_item.setSubmenu(Some(&app_menu));
    let bar = NSMenu::new(mtm);
    bar.addItem(&app_item);
    bar
}

fn window(mtm: MainThreadMarker, scene: &Scene) -> Retained<NSWindow> {
    let size = NSSize::new(scene.size.width, scene.size.height);
    let style =
        NSWindowStyleMask::Titled | NSWindowStyleMask::Closable | NSWindowStyleMask::Miniaturizable;
    // SAFETY: a fresh allocation, initialized once with valid arguments.
    let window = unsafe {
        NSWindow::initWithContentRect_styleMask_backing_defer(
            NSWindow::alloc(mtm),
            NSRect::new(NSPoint::new(0.0, 0.0), size),
            style,
            NSBackingStoreType::Buffered,
            false,
        )
    };
    // SAFETY: `Retained` owns the window, so `AppKit` must not also release it on close.
    unsafe { window.setReleasedWhenClosed(false) };
    window.setTitle(&NSString::from_str(&scene.title));
    window.center();
    if let Some(view) = window.contentView() {
        for label in &scene.labels {
            let field = NSTextField::labelWithString(&NSString::from_str(&label.text), mtm);
            field.setFrame(flipped(scene.size.height, label.frame));
            field.setAlignment(NSTextAlignment::Center);
            // A line longer than its frame keeps both ends: paths end in the part that matters.
            field.setLineBreakMode(NSLineBreakMode::ByTruncatingMiddle);
            field.setTextColor(Some(&color(label.tone)));
            view.addSubview(&field);
        }
    }
    window
}

/// `rect`, whose origin is the top left of a view `height` tall, in `AppKit`'s
/// coordinates, whose origin is the bottom left.
fn flipped(height: f64, rect: Rect) -> NSRect {
    NSRect::new(
        NSPoint::new(rect.x, height - rect.y - rect.height),
        NSSize::new(rect.width, rect.height),
    )
}

fn color(tone: Tone) -> Retained<NSColor> {
    match tone {
        Tone::Primary => NSColor::labelColor(),
        Tone::Secondary => NSColor::secondaryLabelColor(),
        Tone::Error => NSColor::systemRedColor(),
    }
}
