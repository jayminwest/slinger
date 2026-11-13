//! The macOS layer. Every `unsafe` block and every objc2 or `AppKit` call in the
//! project is in this directory (gate check:layers, rule `platform-owns-ffi`). It
//! exposes safe functions and makes no decisions: `input` turns a key press into
//! a [`Chord`], a pen or mouse event into a [`Pen`] (every point: mouse
//! coalescing is off), a trackpad scroll, a right-button drag or a pinch into a
//! pan or a zoom, and a tablet pen's proximity into the end in use, and hands
//! them on, and keeps the pointer's cursor; and it draws what a [`Scene`] says. A
//! canvas view says how long after its pen event it painted new ink (SPEC O2).
//! Each canvas pane is a view of its own (`canvas`) that lives as long as its
//! pane, so a new scene repaints only what changed. `web` shows agent UIs in
//! sandboxed web views, each kept as long as its page is shown. `menu` is the
//! menu bar. `keys` reads a key press, and reads and writes the clipboard for a
//! cut, copy or paste no text field takes (greyed in the Edit menu when the app
//! would do nothing). `entry` is the sling's one-line text field; `overlay` dims
//! the window under the key overlay; `chrome` the focus border and the flash's
//! pill. `text` sets styled text: the sidebar's rows and the panes' footers. The
//! window resizes, down to [`layout::SMALLEST`], and says its new size; `files`
//! sets its title bar, shows a canvas's file in the Finder and hands on files
//! dropped on the window; `pdf` reads and draws PDFs; `panel` asks for a
//! folder or a file to open; `window` makes it; `remember` keeps its frame,
//! whether the sidebar was open and the workspace it showed, for the next
//! launch.
//!
//! Threading rule: `AppKit` runs on the main thread only. Each function here
//! that touches `AppKit` takes a `MainThreadMarker`, which only the main thread
//! can make, so the compiler holds the rule. Blocking work (files, sockets)
//! belongs on another thread; hand its result to the main thread to draw.
#![expect(unsafe_code, reason = "the FFI boundary: AppKit is reached through objc2")]

mod canvas;
mod chrome;
mod entry;
mod files;
mod input;
mod keys;
mod menu;
mod overlay;
mod panel;
pub mod pdf;
mod remember;
mod text;
mod web;
mod window;

use std::cell::{Cell, RefCell};

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObjectProtocol, ProtocolObject};
use objc2::{MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSDragOperation, NSDraggingInfo, NSEvent,
    NSEventType, NSMenuItem, NSResponder, NSTextAlignment, NSView, NSWindow, NSWindowDelegate,
    NSWindowOrderingMode,
};
use objc2_foundation::{NSNotification, NSObject, NSRect, NSTimer};

use self::canvas::CanvasView;
use self::chrome::{color, flipped, label};
pub use self::files::reveal;
pub use self::panel::{Pick, choose};
pub use self::remember::{Kept, remember_sidebar, sidebar as remembered_sidebar};
pub use self::remember::{remember_workspace, workspace as remembered_workspace};
use crate::commands::{Chord, Command};
use crate::layout::{PaneId, Point, Size};
use crate::scene::flash::Pill;
use crate::scene::ink::Pen;
use crate::scene::paste::{Clip, Edit};
use crate::scene::sidebar::Row;
use crate::scene::status::Footer;
use crate::scene::style::Rule;
use crate::scene::{Border, Label, Scene, overlay::Line, overlay::Overlay};

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
    /// The text a key press types, when its [`Event::Key`] was passed on:
    /// as typed, with every modifier (control keys as their characters:
    /// delete, return, escape).
    Text(String),
    /// The pen or pointer moved to a point in the window (top-left origin),
    /// with no button down.
    Moved(Point),
    /// A tablet pen came near the tablet with its eraser end (true), or
    /// with its tip, or left it (false).
    EraserEnd(bool),
    /// The pen or the mouse button went down.
    Press(Pen),
    /// The pen or the mouse moved while down.
    Drag(Pen),
    /// The pen lifted, or the mouse button came up.
    Lift(Pen),
    /// A two-finger scroll (a mouse wheel, a right-button drag) over a point in the window:
    /// drag what is under it by `dx`, `dy` points.
    Scroll { at: Point, dx: f64, dy: f64 },
    /// A pinch, or a scroll with command or control held, over a point in
    /// the window: zoom by `factor` about it.
    Zoom { at: Point, factor: f64 },
    /// An agent UI replied: its `ui_reply` line, for the daemon to hand to
    /// the agent. Answer [`Step::Refuse`] when it cannot go on.
    Reply(String),
    /// A canvas pane painted, this many seconds after the pen event its
    /// scene was the first to show.
    Painted(f64),
    /// The user edited the text of the sling's entry: all of it now.
    Typed(String),
    /// The window's content is this size now: draw a scene of it.
    Resized(Size),
    /// Files dropped on the window at a point; [`Step::Pass`] refuses them.
    Dropped(Point, Vec<std::path::PathBuf>),
    /// A menu item was chosen with the pointer: its command.
    Command(Command),
    /// Cut, copy or paste (the Edit menu or its keys) with no text field
    /// taking it; a paste holds what the clipboard does. Flick's dictation
    /// comes this way, as a paste.
    Edit(Edit),
    /// Whether the app would do this edit: [`Step::Pass`] greys its Edit
    /// menu item, any other step leaves it on.
    Can(Edit),
}

/// What to do about an [`Event`].
pub enum Step {
    /// Nothing: the key is not a command, so the window handles it as usual.
    Pass,
    /// Nothing more.
    Stay,
    Draw(Box<Scene>),
    /// Draw, and let the event go on as with [`Step::Pass`]: a press that
    /// focused an agent UI's pane still reaches its page.
    Through(Box<Scene>),
    Quit,
    /// Put this on the clipboard, and draw.
    Copy(Clip, Box<Scene>),
    /// The reply cannot go on, and why.
    Refuse(String),
}

/// The running app's state: the window, what answers its events, and what
/// it shows.
struct State {
    window: Retained<NSWindow>,
    answer: RefCell<Box<dyn FnMut(Event) -> Step>>,
    shown: RefCell<Shown>,
}

/// The views a window shows, lowest first: one per canvas pane, the chrome
/// (the sidebar, the footers, the focus border, the labels, the flash), the
/// sling's entry, then the overlay (the key overlay, the palette, the page
/// menu), which dims everything under it; and what the chrome and the
/// overlay were made from, so unchanged ones are kept.
#[derive(Default)]
struct Shown {
    canvases: Vec<(PaneId, Retained<CanvasView>)>,
    chrome: Vec<Retained<NSView>>,
    entry: Option<entry::Kept>,
    overlay: Vec<Retained<NSView>>,
    made_from: Option<Chrome>,
}

/// What the chrome is made from: the rules, the sidebar, the footers, the
/// labels and the pills, the border, the overlay and the flash: a part
/// left out would not redraw when only it changed (slinger-7d5db9).
type Chrome =
    (Vec<Rule>, Vec<Row>, Vec<Footer>, Labels, Border, Option<Box<Overlay>>, Option<Box<Pill>>);
type Labels = (Vec<Label>, Vec<Line>);

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

    unsafe impl NSObjectProtocol for KeyWindow {}

    // The window is its own delegate, for these calls.
    unsafe impl NSWindowDelegate for KeyWindow {
        /// The window came to the front: the pointer's cursor is set again.
        #[unsafe(method(windowDidBecomeKey:))]
        fn did_become_key(&self, _note: &NSNotification) {
            input::became_key(self);
        }

        /// The window's content changed size: the scene follows it.
        #[unsafe(method(windowDidResize:))]
        fn did_resize(&self, _note: &NSNotification) {
            let Some(view) = self.contentView() else { return };
            let size = view.frame().size;
            dispatch(Event::Resized(Size { width: size.width, height: size.height }), self.mtm());
        }
    }

    impl KeyWindow {
        /// Files dragged over the window would be copied, if it takes them.
        #[unsafe(method(draggingEntered:))]
        fn drag_over(&self, _info: &ProtocolObject<dyn NSDraggingInfo>) -> NSDragOperation {
            NSDragOperation::Copy
        }

        /// Files dropped go to the app, which takes them or not.
        #[unsafe(method(performDragOperation:))]
        fn drop_files(&self, info: &ProtocolObject<dyn NSDraggingInfo>) -> bool {
            let (at, files) = files::dropped(self, info);
            dispatch(Event::Dropped(at, files), self.mtm())
        }

        /// The timer's action.
        #[unsafe(method(tick:))]
        fn tick(&self, _timer: &NSTimer) {
            dispatch(Event::Tick, self.mtm());
        }

        /// Edit > Paste (or cmd+V) when no text field has the focus: what
        /// the clipboard holds to the app, which may take it or not.
        #[unsafe(method(paste:))]
        fn paste(&self, _sender: Option<&AnyObject>) {
            let clip = keys::clip(self.mtm());
            if !clip.is_empty() {
                dispatch(Event::Edit(Edit::Paste(clip)), self.mtm());
            }
        }

        /// Edit > Copy (or cmd+C) when no text field has the focus.
        #[unsafe(method(copy:))]
        fn copy(&self, _sender: Option<&AnyObject>) {
            dispatch(Event::Edit(Edit::Copy), self.mtm());
        }

        /// Edit > Cut (or cmd+X) when no text field has the focus.
        #[unsafe(method(cut:))]
        fn cut(&self, _sender: Option<&AnyObject>) {
            dispatch(Event::Edit(Edit::Cut), self.mtm());
        }

        /// Cut, copy and paste are on only while the app would do them
        /// ([`Event::Can`]); every other item is the window's to say.
        #[unsafe(method(validateMenuItem:))]
        fn validate_menu_item(&self, item: &NSMenuItem) -> bool {
            let edit = match item.action() {
                Some(a) if a == sel!(cut:) => Edit::Cut,
                Some(a) if a == sel!(copy:) => Edit::Copy,
                Some(a) if a == sel!(paste:) => Edit::Paste(keys::clip(self.mtm())),
                // SAFETY: `NSWindow`'s own `validateMenuItem:`, with its item.
                _ => return unsafe { msg_send![super(self), validateMenuItem: item] },
            };
            dispatch(Event::Can(edit), self.mtm())
        }

        /// The app quits with its window.
        #[unsafe(method(close))]
        fn close_and_quit(&self) {
            // SAFETY: `NSWindow`'s own `close`.
            let () = unsafe { msg_send![super(self), close] };
            NSApplication::sharedApplication(self.mtm()).terminate(None);
        }

        /// Every event for the window comes through here first. A key press,
        /// pen or mouse event, scroll or pinch the app uses stops here, and
        /// keeps a divider's cursor; everything else, moves included, goes on
        /// as usual. After a move the pointer gets the cursor of what is under it.
        #[unsafe(method(sendEvent:))]
        fn send_event(&self, event: &NSEvent) {
            if input::send(self, event, self.mtm()) {
                input::divider();
                return;
            }
            // SAFETY: `NSWindow`'s own `sendEvent:`, with the event it was given.
            let () = unsafe { msg_send![super(self), sendEvent: event] };
            if event.r#type() == NSEventType::MouseMoved {
                input::cursor(self, event.locationInWindow());
            }
        }
    }
);

/// Hands `event` to the app's answer and does what it says. Returns false
/// when the answer is [`Step::Pass`] or [`Step::Through`].
fn dispatch(event: Event, mtm: MainThreadMarker) -> bool {
    let Some(state) = STATE.get() else { return false };
    // An event that comes while the answer runs (none should) is dropped.
    let Ok(mut answer) = state.answer.try_borrow_mut() else { return false };
    let step = answer(event);
    drop(answer);
    match step {
        Step::Pass => return false,
        Step::Through(scene) => {
            draw(state, &scene, mtm);
            return false;
        }
        Step::Stay | Step::Refuse(_) => {}
        Step::Draw(scene) => draw(state, &scene, mtm),
        Step::Copy(clip, scene) => {
            keys::copy(&clip, mtm);
            draw(state, &scene, mtm);
        }
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
    // A reply that comes while the answer runs (an open panel is up) waits for none.
    let Ok(mut answer) = state.answer.try_borrow_mut() else {
        return Err("the app is busy: a panel is open".into());
    };
    let step = answer(Event::Reply(line));
    drop(answer);
    match step {
        Step::Refuse(why) => return Err(why),
        Step::Draw(scene) | Step::Through(scene) | Step::Copy(_, scene) => {
            draw(state, &scene, mtm);
        }
        Step::Quit => state.window.close(),
        Step::Pass | Step::Stay => {}
    }
    Ok(())
}

/// Shows `scene` in a window and runs the app until it quits (the window
/// closes, or a [`Step::Quit`]). Each key press and twice a second a tick go
/// to `answer` on the main thread, which must return quickly; so do pen and
/// mouse presses, drags, lifts and moves in the window, and each paint of
/// new ink. The window opens at, and keeps, the `kept` frame. Call it from
/// `main`.
///
/// # Errors
///
/// [`NotMainThread`] when called from any other thread.
pub fn run(
    scene: &Scene,
    kept: Kept,
    answer: impl FnMut(Event) -> Step + 'static,
) -> Result<(), NotMainThread> {
    let mtm = MainThreadMarker::new().ok_or(NotMainThread)?;
    // Texts are hit where their glyphs are, as this thread lays them out.
    crate::scene::glyphs::set_measure(canvas::measure);
    let app = NSApplication::sharedApplication(mtm);
    // A run in the background has no Dock icon and never comes forward.
    app.setActivationPolicy(if kept.front() {
        NSApplicationActivationPolicy::Regular
    } else {
        NSApplicationActivationPolicy::Accessory
    });
    // Every pen point, not one per frame: a fast stroke keeps its shape.
    NSEvent::setMouseCoalescingEnabled(false);
    let window = window::make(mtm, scene.size, kept);
    files::take_drops(&window);
    chrome::tick_after(&window, POLL_SECONDS, true);
    let window = Retained::into_super(window);
    let answer = RefCell::new(Box::new(answer) as Box<dyn FnMut(Event) -> Step>);
    let shown = RefCell::new(Shown::default());
    // The app ends in `exit`, so the state lives as long as it does.
    let state = Box::leak(Box::new(State { window, answer, shown }));
    draw(state, scene, mtm);
    STATE.set(Some(state));
    // The window opens at its saved size: the first scene follows it.
    let opened = state.window.contentView().map(|view| view.frame().size);
    if let Some(size) =
        opened.filter(|s| (s.width, s.height) != (scene.size.width, scene.size.height))
    {
        dispatch(Event::Resized(Size { width: size.width, height: size.height }), mtm);
    }
    if kept.front() {
        state.window.makeKeyAndOrderFront(None);
        app.activate();
    }
    app.run();
    Ok(())
}

/// Makes the window show `scene`: its title, background, canvas panes,
/// sidebar, footers, labels, border and overlay. Canvas views stay from scene
/// to scene; the chrome is made again only when it changed.
fn draw(state: &State, scene: &Scene, mtm: MainThreadMarker) {
    let window = &state.window;
    files::title(window, &scene.title);
    window.setBackgroundColor(Some(&color(scene.theme.background)));
    menu::show(&scene.menus, mtm);
    let Some(view) = window.contentView() else { return };
    let shown = &mut *state.shown.borrow_mut();
    let height = scene.size.height;
    input::DIVIDER.set(scene.divider);

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
        canvas.show(frame, pane.scene.clone(), pane.cursor);
    }
    let canvas = |pane| {
        let found = shown.canvases.iter().find(|(p, _)| *p == pane);
        found.map(|(_, canvas)| Retained::into_super(canvas.clone()))
    };
    web::place(&view, &scene.webs, &scene.theme, height, canvas, mtm);
    let under = shown.overlay.first().cloned();
    entry::place(window, &view, &mut shown.entry, scene, under.as_deref(), mtm);

    let made_from = Some((
        scene.rules.clone(),
        scene.sidebar.clone(),
        scene.footers.clone(),
        (scene.labels.clone(), scene.pills.clone()),
        scene.focus,
        scene.overlay.clone(),
        scene.flash.clone(),
    ));
    if shown.made_from == made_from {
        return;
    }
    for old in shown.chrome.drain(..).chain(shown.overlay.drain(..)) {
        old.removeFromSuperview();
    }
    for rule in &scene.rules {
        let block = Label {
            text: String::new(),
            colour: rule.colour,
            fill: Some(rule.colour),
            frame: rule.frame,
        };
        let field = label(&block, height, NSTextAlignment::Left, mtm);
        shown.chrome.push(Retained::into_super(Retained::into_super(field)));
    }
    for row in &scene.sidebar {
        let field = text::sidebar_row(row, height, mtm);
        shown.chrome.push(Retained::into_super(Retained::into_super(field)));
    }
    for footer in &scene.footers {
        shown.chrome.extend(text::footer(footer, height, mtm));
    }
    // The border first, so a pop-up at the pen draws over it.
    shown.chrome.push(Retained::into_super(chrome::border(&scene.focus, height, mtm)));
    shown.chrome.extend(scene.labels.iter().flat_map(|line| text::line(line, height, mtm)));
    shown.chrome.extend(scene.pills.iter().flat_map(|line| overlay::line(line, height, mtm)));
    if let Some(pill) = &scene.flash {
        shown.chrome.push(Retained::into_super(chrome::pill(window, pill, height, mtm)));
    }
    for chrome in &shown.chrome {
        // Under the sling's entry, if it is up: only the overlay is over it.
        match &shown.entry {
            Some(entry) => {
                view.addSubview_positioned_relativeTo(
                    chrome,
                    NSWindowOrderingMode::Below,
                    Some(entry.bottom()),
                );
            }
            None => view.addSubview(chrome),
        }
    }
    shown.overlay = scene.overlay.iter().flat_map(|o| overlay::views(o, height, mtm)).collect();
    shown.overlay.iter().for_each(|over| view.addSubview(over));
    shown.made_from = made_from;
}
