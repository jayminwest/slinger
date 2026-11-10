//! The macOS layer. Every `unsafe` block and every objc2 or `AppKit` call in the
//! project is in this directory (gate check:layers, rule `platform-owns-ffi`).
//! It exposes safe functions and makes no decisions: it turns a key press
//! into a [`Chord`] and hands it on, and it draws what a [`Scene`] says.
//!
//! Threading rule: `AppKit` runs on the main thread only. Each function here
//! that touches `AppKit` takes a `MainThreadMarker`, which only the main thread
//! can make, so the compiler holds the rule. Blocking work (files, sockets)
//! belongs on another thread; hand its result to the main thread to draw.
#![expect(unsafe_code, reason = "the FFI boundary: AppKit is reached through objc2")]

use std::cell::{Cell, RefCell};

use objc2::rc::Retained;
use objc2::{MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSBackingStoreType, NSColor, NSEvent,
    NSEventType, NSLineBreakMode, NSResponder, NSTextAlignment, NSTextField, NSWindow,
    NSWindowStyleMask,
};
use objc2_foundation::{NSArray, NSObject, NSPoint, NSRect, NSSize, NSString, NSTimer};

use crate::commands::Chord;
use crate::config::Rgb;
use crate::layout::Rect;
use crate::scene::Scene;

/// How often the main thread looks for a new scene, in seconds.
const POLL_SECONDS: f64 = 0.5;

/// [`run`] was called off the main thread.
#[derive(Debug)]
pub struct NotMainThread;

impl std::fmt::Display for NotMainThread {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("the app must start on the main thread")
    }
}

/// What the app hears.
pub enum Event {
    /// The timer: twice a second.
    Tick,
    /// A key press.
    Key(Chord),
}

/// What to do about an [`Event`].
pub enum Step {
    /// Nothing: the key is not a command, so the window handles it as usual.
    Pass,
    /// Nothing more.
    Stay,
    Draw(Scene),
    Quit,
}

/// The running app's state: the window, and what answers its events.
struct State {
    window: Retained<NSWindow>,
    answer: RefCell<Box<dyn FnMut(Event) -> Step>>,
}

thread_local! {
    /// Set once by [`run`], on the main thread, and kept for the app's life.
    static STATE: Cell<Option<&'static State>> = const { Cell::new(None) };
}

define_class!(
    // SAFETY: `NSWindow` may be subclassed, and `KeyWindow` has no ivars and
    // no `Drop` impl.
    #[unsafe(super(NSWindow, NSResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "SlingerWindow"]
    struct KeyWindow;

    impl KeyWindow {
        /// The timer's action.
        #[unsafe(method(tick:))]
        fn tick(&self, _timer: &NSTimer) {
            dispatch(Event::Tick, self.mtm());
        }

        /// The app quits with its window.
        #[unsafe(method(close))]
        fn close_and_quit(&self) {
            // SAFETY: `NSWindow`'s own `close`.
            let () = unsafe { msg_send![super(self), close] };
            NSApplication::sharedApplication(self.mtm()).terminate(None);
        }

        /// Every event for the window comes through here first. A key press
        /// that is a command stops here; everything else goes on as usual.
        #[unsafe(method(sendEvent:))]
        fn send_event(&self, event: &NSEvent) {
            if event.r#type() == NSEventType::KeyDown
                && let Some(chord) = chord(event)
                && dispatch(Event::Key(chord), self.mtm())
            {
                return;
            }
            // SAFETY: `NSWindow`'s own `sendEvent:`, with the event it was given.
            let () = unsafe { msg_send![super(self), sendEvent: event] };
        }
    }
);

/// The chord of a key press: the device-independent modifier flags, whose
/// shift, control, option and command bits (17 to 20) are the [`Chord`]
/// bits in order, and the key's character with no modifier but shift.
fn chord(event: &NSEvent) -> Option<Chord> {
    let chars = event.charactersIgnoringModifiers()?;
    if chars.length() == 0 {
        return None;
    }
    let key = char::from_u32(chars.characterAtIndex(0).into())?;
    #[expect(clippy::cast_possible_truncation, reason = "masked to four bits")]
    let mods = ((event.modifierFlags().0 >> 17) & 0xf) as u8;
    Some(Chord::new(mods, key))
}

/// Hands `event` to the app's answer and does what it says. Returns false
/// when the answer is [`Step::Pass`].
fn dispatch(event: Event, mtm: MainThreadMarker) -> bool {
    let Some(state) = STATE.get() else { return false };
    let step = (state.answer.borrow_mut())(event);
    match step {
        Step::Pass => return false,
        Step::Stay => {}
        Step::Draw(scene) => draw(&state.window, &scene, mtm),
        // The window quits the app as it closes.
        Step::Quit => state.window.close(),
    }
    true
}

/// Shows `scene` in a window and runs the app until it quits (the window
/// closes, or a [`Step::Quit`]). Each key press and twice a second a tick go
/// to `answer` on the main thread, which must return quickly. Call it from
/// `main`.
///
/// # Errors
///
/// [`NotMainThread`] when called from any other thread.
pub fn run(
    scene: &Scene,
    answer: impl FnMut(Event) -> Step + 'static,
) -> Result<(), NotMainThread> {
    let mtm = MainThreadMarker::new().ok_or(NotMainThread)?;
    let app = NSApplication::sharedApplication(mtm);
    app.setActivationPolicy(NSApplicationActivationPolicy::Regular);
    let window = window(mtm, scene);
    draw(&window, scene, mtm);
    window.makeKeyAndOrderFront(None);
    // SAFETY: the target implements `tick:`, which takes the timer. The run
    // loop keeps the repeating timer, and the timer keeps its target.
    let _timer = unsafe {
        NSTimer::scheduledTimerWithTimeInterval_target_selector_userInfo_repeats(
            POLL_SECONDS,
            &window,
            sel!(tick:),
            None,
            true,
        )
    };
    let window = Retained::into_super(window);
    // The app ends in `exit`, so the state lives as long as it does.
    let state = Box::leak(Box::new(State { window, answer: RefCell::new(Box::new(answer)) }));
    STATE.set(Some(state));
    app.activate();
    app.run();
    Ok(())
}

fn window(mtm: MainThreadMarker, scene: &Scene) -> Retained<KeyWindow> {
    let size = NSSize::new(scene.size.width, scene.size.height);
    let style =
        NSWindowStyleMask::Titled | NSWindowStyleMask::Closable | NSWindowStyleMask::Miniaturizable;
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
    window.center();
    window
}

/// Makes `window` show `scene`: its title, background and labels.
fn draw(window: &NSWindow, scene: &Scene, mtm: MainThreadMarker) {
    window.setTitle(&NSString::from_str(&scene.title));
    window.setBackgroundColor(Some(&color(scene.background)));
    let Some(view) = window.contentView() else { return };
    view.setSubviews(&NSArray::new());
    for label in &scene.labels {
        let field = NSTextField::labelWithString(&NSString::from_str(&label.text), mtm);
        field.setFrame(flipped(scene.size.height, label.frame));
        field.setAlignment(NSTextAlignment::Center);
        // A line longer than its frame keeps both ends: paths end in the part that matters.
        field.setLineBreakMode(NSLineBreakMode::ByTruncatingMiddle);
        field.setTextColor(Some(&color(label.colour)));
        view.addSubview(&field);
    }
}

/// `rect`, whose origin is the top left of a view `height` tall, in `AppKit`'s
/// coordinates, whose origin is the bottom left.
fn flipped(height: f64, rect: Rect) -> NSRect {
    NSRect::new(
        NSPoint::new(rect.x, height - rect.y - rect.height),
        NSSize::new(rect.width, rect.height),
    )
}

fn color(rgb: Rgb) -> Retained<NSColor> {
    let channel = |c: u8| f64::from(c) / 255.0;
    NSColor::colorWithSRGBRed_green_blue_alpha(channel(rgb.r), channel(rgb.g), channel(rgb.b), 1.0)
}
