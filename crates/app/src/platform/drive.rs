//! The driven window's side (`slinger-app --drive`, slinger-338dfc): a request from the driver's thread runs on the main
//! thread, where its events go in as the window's own do ([`dispatch`]);
//! the last scene drawn is kept to say as text; the window's content is
//! drawn into a PNG, on a screen or not; a cut or a copy is kept here,
//! never on the user's clipboard; and a context menu is kept to say and to
//! pick from, never popped (any input but a tick closes it).

use std::cell::RefCell;
use std::ffi::c_void;
use std::path::Path;
use std::sync::mpsc;

use objc2::runtime::{AnyClass, Sel};
use objc2::{MainThreadMarker, msg_send};
use objc2_app_kit::{NSBitmapImageFileType, NSWindow};
use objc2_foundation::{NSDictionary, NSSize, NSString};

use super::event::{_dispatch_main_q, dispatch_async_f};
use super::{Event, STATE, Step, dispatch};
use crate::commands::context::Popup;
use crate::commands::menu::{Action, Native};
use crate::drive::{self, Ask, Field};
use crate::scene::Scene;
use crate::scene::paste::{Clip, Edit};

thread_local! {
    /// The last scene the driven window drew.
    static LAST: RefCell<Option<Box<Scene>>> = const { RefCell::new(None) };
    /// What the driven window last cut or copied, in place of the clipboard.
    static CLIP: RefCell<Clip> = RefCell::new(Clip::default());
    /// The context menu the driven window would have popped, until the
    /// next event closes it, as a click outside a real one does.
    static MENU: RefCell<Option<Popup>> = const { RefCell::new(None) };
}

/// A request and where its answer goes.
type Job = (Ask, mpsc::Sender<Result<String, String>>);

/// `answer`, keeping the scene the window shows (`first`, each it draws and
/// the text typed in an entry: [`drive::shown`]) for [`Ask::Scene`].
pub fn watching(
    first: &Scene,
    mut answer: impl FnMut(Event) -> Step + 'static,
) -> impl FnMut(Event) -> Step + 'static {
    LAST.set(Some(Box::new(first.clone())));
    move |event| {
        let typed = match &event {
            Event::Typed(text) => Some(text.clone()),
            _ => None,
        };
        // A driven window never pops a menu: it would hold the main thread
        // until a person closed it. It keeps the menu to say and pick from.
        let step = match answer(event) {
            Step::Menu(popup, scene) => {
                MENU.set(Some(popup));
                Step::Draw(scene)
            }
            step => step,
        };
        LAST.with_borrow_mut(|last| {
            if let Some(last) = last {
                drive::shown(last, typed, &step);
            }
        });
        // A cut or a copy stays in the driven window: the user's clipboard is theirs.
        match step {
            Step::Copy(clip, scene) => {
                CLIP.set(clip);
                Step::Draw(scene)
            }
            step => step,
        }
    }
}

/// Does what `ask` asks on the main thread and waits for what it said.
/// Call it from any other thread.
///
/// # Errors
///
/// Why the window could not do it.
pub fn ask(ask: Ask) -> Result<String, String> {
    let (said, heard) = mpsc::channel();
    let job: *mut Job = Box::into_raw(Box::new((ask, said)));
    // SAFETY: the main queue lives as long as the process; `run` takes the
    // box back exactly once.
    unsafe { dispatch_async_f(&raw const _dispatch_main_q, job.cast(), run) };
    heard.recv().map_err(|_| "the app quit".to_string())?
}

/// Runs one [`Job`] on the main thread.
extern "C" fn run(job: *mut c_void) {
    // SAFETY: `job` is the box [`ask`] leaked, handed here once.
    let (ask, said) = *unsafe { Box::from_raw(job.cast::<Job>()) };
    let Some(mtm) = MainThreadMarker::new() else { return };
    // The driver that went away hears nothing.
    let _ = said.send(answer(ask, mtm));
}

/// What the window says to `ask`: for events, `taken` or `passed` each.
fn answer(ask: Ask, mtm: MainThreadMarker) -> Result<String, String> {
    let state = STATE.get().ok_or("the app is not running")?;
    match ask {
        Ask::Send(events) => {
            // Any input closes the menu, as a click outside a real one does; the timer does not.
            if events.iter().any(|(event, _)| *event != Event::Tick) {
                MENU.take();
            }
            let mut said = Vec::new();
            for (event, text) in events {
                let mut took = dispatch(event, mtm);
                if let Some(text) = text.filter(|_| !took) {
                    took = dispatch(Event::Text(text.clone()), mtm)
                        || into_field(&state.window, &text);
                }
                said.push(if took { "taken" } else { "passed" });
            }
            Ok(said.join(" "))
        }
        Ask::Scene { debug } => LAST
            .with_borrow(|scene| {
                let scene = scene.as_ref()?;
                Some(if debug { format!("{scene:#?}") } else { crate::drive::outline(scene) })
            })
            .map(|said| {
                said + &MENU.with_borrow(|m| m.as_ref().map(drive::menu)).unwrap_or_default()
            })
            .ok_or_else(|| "no scene yet".to_string()),
        Ask::Pick(title) => {
            let popup = MENU.take();
            let action = drive::pick(popup.as_ref(), &title)?;
            let event = match (action, popup) {
                (Action::Run(command), Some(popup)) => Event::Picked(command, popup.subject),
                (Action::Run(command), None) => Event::Command(command),
                (Action::Native(Native::Paste), _) => {
                    Event::Edit(Edit::Paste(CLIP.with_borrow(Clone::clone)))
                }
                (Action::Native(Native::Copy), _) => Event::Edit(Edit::Copy),
                (Action::Native(Native::Cut), _) => Event::Edit(Edit::Cut),
                _ => {
                    return Err(format!("{title:?} is macOS's own: a driven window cannot run it"));
                }
            };
            Ok(if dispatch(event, mtm) { "taken" } else { "passed" }.to_string())
        }
        Ask::Paste => {
            MENU.take();
            let took = dispatch(Event::Edit(Edit::Paste(CLIP.with_borrow(Clone::clone))), mtm);
            Ok(if took { "taken" } else { "passed" }.to_string())
        }
        Ask::Clip => Ok(CLIP.with_borrow(|clip| clip.text.clone().unwrap_or_default())),
        Ask::Resize(size) => {
            state.window.setContentSize(NSSize::new(size.width, size.height));
            Ok(String::new())
        }
        Ask::Shot(path) => shot(&state.window, &path),
    }
}

/// Types `text` into the entry's field when it holds `window`'s keyboard,
/// as a key press does through the field editor: the app passes typing on
/// to it, and a driven window gets no key events from the system. A key
/// that types no character runs its command instead ([`drive::into_field`]).
fn into_field(window: &NSWindow, text: &str) -> bool {
    let Some(class) = AnyClass::get(c"NSTextView") else { return false };
    let Some(editor) = window.firstResponder() else { return false };
    // SAFETY: `isKindOfClass:` takes any class.
    let editing: bool = unsafe { msg_send![&*editor, isKindOfClass: class] };
    if !editing {
        return false;
    }
    match drive::into_field(text) {
        // SAFETY: an `NSTextView` (the field editor) answers `insertText:` with a string.
        Field::Insert(text) => unsafe {
            let () = msg_send![&*editor, insertText: &*NSString::from_str(text)];
        },
        // SAFETY: `doCommandBySelector:` takes any selector; one the field
        // editor does not know only beeps.
        Field::Command(name) => unsafe {
            let () = msg_send![&*editor, doCommandBySelector: Sel::register(name)];
        },
        Field::Nothing => return false,
    }
    true
}

/// Draws the content of `window` into a PNG at `path`.
fn shot(window: &NSWindow, path: &Path) -> Result<String, String> {
    let view = window.contentView().ok_or("the window has no content")?;
    let bounds = view.bounds();
    view.displayIfNeeded();
    let image = view.bitmapImageRepForCachingDisplayInRect(bounds).ok_or("no bitmap")?;
    view.cacheDisplayInRect_toBitmapImageRep(bounds, &image);
    // SAFETY: no properties: the dictionary is empty.
    let png = unsafe {
        image.representationUsingType_properties(NSBitmapImageFileType::PNG, &NSDictionary::new())
    };
    let png = png.ok_or("the bitmap has no PNG form")?;
    std::fs::write(path, png.to_vec()).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(path.display().to_string())
}
