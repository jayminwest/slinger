//! What the app hears ([`Event`]) and what it answers ([`Step`]); and the
//! one way another thread asks the main thread to look at the news
//! ([`waker`]).

use std::ffi::c_void;

use objc2::MainThreadMarker;

use super::dispatch;
use crate::commands::{Chord, Command};
use crate::layout::{Point, Size};
use crate::scene::Scene;
use crate::scene::ink::Pen;
use crate::scene::paste::{Clip, Edit};

unsafe extern "C" {
    /// The main thread's queue, which `AppKit`'s run loop serves in its
    /// common modes: during a live resize, a drag and a stroke too.
    pub(super) static _dispatch_main_q: c_void;
    pub(super) fn dispatch_async_f(
        queue: *const c_void,
        context: *mut c_void,
        work: extern "C" fn(*mut c_void),
    );
}

/// The function that asks the main thread for an [`Event::Wake`]; any
/// thread may call it. A wake that comes while the app's answer runs is
/// dropped, as every such event is: the timer is the backstop.
pub fn waker() -> fn() {
    post
}

/// Queues a [`Event::Wake`] on the main thread.
fn post() {
    // SAFETY: the main queue lives as long as the process; `woken` takes no
    // context.
    unsafe { dispatch_async_f(&raw const _dispatch_main_q, std::ptr::null_mut(), woken) };
}

/// Runs on the main thread: the app looks at the news.
extern "C" fn woken(_context: *mut c_void) {
    if let Some(mtm) = MainThreadMarker::new() {
        dispatch(Event::Wake, mtm);
    }
}

/// What the app hears.
#[derive(Debug, PartialEq)]
pub enum Event {
    /// The timer: twice a second.
    Tick,
    /// A backend thread has news ([`waker`]).
    Wake,
    /// The app is about to quit: its window closed (the red button, or a
    /// [`Step::Quit`]). The answer may wait a moment, never long.
    Quitting,
    /// A key press.
    Key(Chord),
    /// The text a key press types, when its [`Event::Key`] was passed on:
    /// as typed, with every modifier (control keys as their characters:
    /// delete, return, escape).
    Text(String),
    /// The pen or pointer moved to a point in the window (top-left origin),
    /// with no button down.
    Moved(Point),
    /// A tablet pen came near the tablet (`near`) or left it, with its eraser end or not.
    Proximity { near: bool, eraser: bool },
    /// Space, no modifier, went down (true, and each repeat) or up; or the keys went.
    Space(bool),
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
