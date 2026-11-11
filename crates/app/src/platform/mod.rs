//! The macOS layer. Every `unsafe` block and every objc2 or `AppKit` call in the
//! project is in this directory (gate check:layers, rule `platform-owns-ffi`).
//! It exposes safe functions and makes no decisions: it turns a key press
//! into a [`Chord`], a pen or mouse event into a [`Pen`] (every point: mouse
//! coalescing is off) and a trackpad scroll or pinch into a pan or a zoom,
//! and hands them on, and it draws what a [`Scene`] says. A canvas view
//! says how long after its pen event it painted new ink (SPEC O2). Each
//! canvas pane is a view of its own (`canvas`) that lives as long as its
//! pane, so a new scene repaints only what changed.
//! `web` shows agent UIs in sandboxed web views, each kept as long as its
//! page is shown.
//!
//! Threading rule: `AppKit` runs on the main thread only. Each function here
//! that touches `AppKit` takes a `MainThreadMarker`, which only the main thread
//! can make, so the compiler holds the rule. Blocking work (files, sockets)
//! belongs on another thread; hand its result to the main thread to draw.
#![expect(unsafe_code, reason = "the FFI boundary: AppKit is reached through objc2")]

mod canvas;
mod web;

use std::cell::{Cell, RefCell};

use objc2::rc::Retained;
use objc2::{MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSBackingStoreType, NSBox, NSBoxType, NSColor,
    NSEvent, NSEventSubtype, NSEventType, NSLineBreakMode, NSResponder, NSTextAlignment,
    NSTextField, NSTitlePosition, NSView, NSWindow, NSWindowOrderingMode, NSWindowStyleMask,
};
use objc2_foundation::{NSObject, NSPoint, NSRect, NSSize, NSString, NSTimer};

use self::canvas::CanvasView;
use crate::commands::Chord;
use crate::config::Rgb;
use crate::layout::{PaneId, Point, Rect};
use crate::scene::ink::Pen;
use crate::scene::{Border, Label, Scene};

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
    /// The pen or pointer moved to a point in the window (top-left origin),
    /// with no button down.
    Moved(Point),
    /// The pen or the mouse button went down.
    Press(Pen),
    /// The pen or the mouse moved while down.
    Drag(Pen),
    /// The pen lifted, or the mouse button came up.
    Lift(Pen),
    /// A two-finger scroll (or a mouse wheel) over a point in the window:
    /// drag what is under it by `dx`, `dy` points.
    Scroll { at: Point, dx: f64, dy: f64 },
    /// A pinch over a point in the window: zoom by `factor` about it.
    Zoom { at: Point, factor: f64 },
    /// An agent UI replied: its `ui_reply` line, for the daemon to hand to
    /// the agent. Answer [`Step::Refuse`] when it cannot go on.
    Reply(String),
    /// A canvas pane painted, this many seconds after the pen event its
    /// scene was the first to show.
    Painted(f64),
}

/// What to do about an [`Event`].
pub enum Step {
    /// Nothing: the key is not a command, so the window handles it as usual.
    Pass,
    /// Nothing more.
    Stay,
    Draw(Scene),
    Quit,
    /// The reply cannot go on, and why.
    Refuse(String),
}

/// How far one line of a mouse wheel scrolls, in points. A trackpad gives points.
const WHEEL_LINE: f64 = 16.0;

/// The running app's state: the window, what answers its events, and what
/// it shows.
struct State {
    window: Retained<NSWindow>,
    answer: RefCell<Box<dyn FnMut(Event) -> Step>>,
    shown: RefCell<Shown>,
}

/// The views a window shows: one per canvas pane, under the chrome (the
/// sidebar, the focus border and the labels), and what the chrome was made from, so an
/// unchanged one is kept.
#[derive(Default)]
struct Shown {
    canvases: Vec<(PaneId, Retained<CanvasView>)>,
    chrome: Vec<Retained<NSView>>,
    made_from: Option<(Vec<Label>, Vec<Label>, Border)>,
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

        /// Every event for the window comes through here first. A key press,
        /// pen or mouse event, scroll or pinch the app uses stops here;
        /// everything else, moves included, goes on as usual.
        #[unsafe(method(sendEvent:))]
        fn send_event(&self, event: &NSEvent) {
            let kind = event.r#type();
            let used = if kind == NSEventType::KeyDown {
                chord(event).is_some_and(|chord| dispatch(Event::Key(chord), self.mtm()))
            } else if kind == NSEventType::LeftMouseDown {
                dispatch(Event::Press(self.pen(event)), self.mtm())
            } else if kind == NSEventType::LeftMouseDragged {
                dispatch(Event::Drag(self.pen(event)), self.mtm())
            } else if kind == NSEventType::LeftMouseUp {
                dispatch(Event::Lift(self.pen(event)), self.mtm())
            } else if kind == NSEventType::ScrollWheel {
                let unit = if event.hasPreciseScrollingDeltas() { 1.0 } else { WHEEL_LINE };
                let (dx, dy) = (event.scrollingDeltaX() * unit, event.scrollingDeltaY() * unit);
                dispatch(Event::Scroll { at: self.point(event), dx, dy }, self.mtm())
            } else if kind == NSEventType::Magnify {
                let factor = 1.0 + event.magnification();
                dispatch(Event::Zoom { at: self.point(event), factor }, self.mtm())
            } else {
                if kind == NSEventType::MouseMoved {
                    dispatch(Event::Moved(self.point(event)), self.mtm());
                }
                false
            };
            if used {
                return;
            }
            // SAFETY: `NSWindow`'s own `sendEvent:`, with the event it was given.
            let () = unsafe { msg_send![super(self), sendEvent: event] };
        }
    }
);

impl KeyWindow {
    /// Where a pen or mouse event is in the window's content, from the top left.
    fn point(&self, event: &NSEvent) -> Point {
        let at = event.locationInWindow();
        let height = self.contentView().map_or(0.0, |v| v.frame().size.height);
        Point { x: at.x, y: height - at.y }
    }

    /// A pen or mouse event: where, when, and whether a finger made it.
    fn pen(&self, event: &NSEvent) -> Pen {
        let touch = event.subtype() == NSEventSubtype::Touch;
        Pen { at: self.point(event), when: event.timestamp(), touch }
    }
}

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
    // An event that comes while the answer runs (none should) is dropped.
    let Ok(mut answer) = state.answer.try_borrow_mut() else { return false };
    let step = answer(event);
    drop(answer);
    match step {
        Step::Pass => return false,
        Step::Stay | Step::Refuse(_) => {}
        Step::Draw(scene) => draw(state, &scene, mtm),
        // The window quits the app as it closes.
        Step::Quit => state.window.close(),
    }
    true
}

/// Hands an agent UI's reply `line` to the app's answer.
///
/// # Errors
///
/// Why the answer refused it.
fn reply(line: String, mtm: MainThreadMarker) -> Result<(), String> {
    let Some(state) = STATE.get() else { return Err("the app is not running".into()) };
    let step = (state.answer.borrow_mut())(Event::Reply(line));
    match step {
        Step::Refuse(why) => return Err(why),
        Step::Draw(scene) => draw(state, &scene, mtm),
        Step::Quit => state.window.close(),
        Step::Pass | Step::Stay => {}
    }
    Ok(())
}

/// Shows `scene` in a window and runs the app until it quits (the window
/// closes, or a [`Step::Quit`]). Each key press and twice a second a tick go
/// to `answer` on the main thread, which must return quickly; so do pen and
/// mouse presses, drags, lifts and moves in the window, and each paint of
/// new ink. Call it from `main`.
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
    // Every pen point, not one per frame: a fast stroke keeps its shape.
    NSEvent::setMouseCoalescingEnabled(false);
    let window = window(mtm, scene);
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
    let answer = RefCell::new(Box::new(answer) as Box<dyn FnMut(Event) -> Step>);
    let shown = RefCell::new(Shown::default());
    // The app ends in `exit`, so the state lives as long as it does.
    let state = Box::leak(Box::new(State { window, answer, shown }));
    draw(state, scene, mtm);
    state.window.makeKeyAndOrderFront(None);
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
    window.setAcceptsMouseMovedEvents(true);
    window.center();
    window
}

/// Makes the window show `scene`: its title, background, canvas panes,
/// sidebar, labels and border. Canvas views stay from scene to scene; the chrome is
/// made again only when it changed.
fn draw(state: &State, scene: &Scene, mtm: MainThreadMarker) {
    let window = &state.window;
    let title = NSString::from_str(&scene.title);
    if window.title() != title {
        window.setTitle(&title);
    }
    window.setBackgroundColor(Some(&color(scene.background)));
    let Some(view) = window.contentView() else { return };
    let mut shown = state.shown.borrow_mut();
    let height = scene.size.height;

    shown.canvases.retain(|(pane, canvas)| {
        let kept = scene.canvases.iter().any(|c| c.pane == *pane);
        if !kept {
            canvas.removeFromSuperview();
        }
        kept
    });
    for pane in &scene.canvases {
        let frame = flipped(height, pane.frame);
        let found = shown.canvases.iter().find(|(p, _)| *p == pane.pane);
        let canvas = if let Some((_, canvas)) = found {
            canvas.clone()
        } else {
            let canvas = CanvasView::new(frame, mtm);
            // Below every other view: the chrome draws over the canvases.
            view.addSubview_positioned_relativeTo(&canvas, NSWindowOrderingMode::Below, None);
            shown.canvases.push((pane.pane, canvas.clone()));
            canvas
        };
        canvas.show(frame, pane.scene.clone());
    }
    web::place(&view, &scene.webs, height, mtm);

    let made_from = Some((scene.sidebar.clone(), scene.labels.clone(), scene.focus));
    if shown.made_from == made_from {
        return;
    }
    for old in shown.chrome.drain(..) {
        old.removeFromSuperview();
    }
    for row in &scene.sidebar {
        let field = label(row, height, NSTextAlignment::Left, mtm);
        field.setLineBreakMode(NSLineBreakMode::ByTruncatingTail);
        shown.chrome.push(Retained::into_super(Retained::into_super(field)));
    }
    // The border first, so a pop-up at the pen draws over it.
    shown.chrome.push(Retained::into_super(border(&scene.focus, height, mtm)));
    for line in &scene.labels {
        let field = label(line, height, NSTextAlignment::Center, mtm);
        // A line longer than its frame keeps both ends: paths end in the part that matters.
        field.setLineBreakMode(NSLineBreakMode::ByTruncatingMiddle);
        shown.chrome.push(Retained::into_super(Retained::into_super(field)));
    }
    for chrome in &shown.chrome {
        view.addSubview(chrome);
    }
    shown.made_from = made_from;
}

/// A text field that shows `line` in a view `height` tall.
fn label(
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

/// A box that draws only its edge: no title, no fill.
fn border(border: &Border, height: f64, mtm: MainThreadMarker) -> Retained<NSBox> {
    let edge = NSBox::new(mtm);
    edge.setBoxType(NSBoxType::Custom);
    edge.setTitlePosition(NSTitlePosition::NoTitle);
    edge.setBorderWidth(border.width);
    edge.setBorderColor(&color(border.colour));
    edge.setFillColor(&NSColor::clearColor());
    edge.setFrame(flipped(height, border.frame));
    edge
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
