//! The menu bar, made as `commands::menu` says, and made again when that
//! changes (a reloaded keymap, a new tool). A command item goes to the app's
//! answer: chosen by its key, as that key press, so the keymap runs it once;
//! chosen with the pointer, as [`Event::Command`]. A native item goes to the
//! window or the text field with the focus (cut, copy, paste). The menu
//! takes its keys before any field: an undo, a redo or a select all, by its
//! key or the pointer, that the app passes on (an entry or a page has the
//! keys) goes to the field as `undo:`, `redo:` or `selectAll:` along the
//! responder chain, so it acts on the typing there.
//!
//! A command item is grey where the newest model says (`Item::enabled`):
//! `AppKit` asks the target (`validateMenuItem:`) as a menu opens and
//! before an item's key fires it. A model that changes only in that
//! ([`same_layout`]) keeps the made menus, so an open menu is not made
//! again under the pointer.

use std::cell::RefCell;

use objc2::rc::Retained;
use objc2::runtime::{NSObjectProtocol, Sel};
use objc2::{MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{
    NSApplication, NSControlStateValueOn, NSEventModifierFlags, NSEventType, NSMenu, NSMenuItem,
};
use objc2_foundation::{NSObject, NSString};

use super::keys::chord;
use super::{Event, dispatch};
use crate::commands::Command;
use crate::commands::menu::{Action, Item, Menu, Native, same_layout};

define_class!(
    // SAFETY: `NSObject` has no subclassing requirements, and `Target` has
    // no ivars and no `Drop` impl.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "SlingerMenuTarget"]
    struct Target;

    unsafe impl NSObjectProtocol for Target {}

    impl Target {
        /// A command item's action: its tag is the command's index.
        #[unsafe(method(run:))]
        fn run(&self, item: &NSMenuItem) {
            let mtm = self.mtm();
            let Some(&(command, _)) = usize::try_from(item.tag()).ok().and_then(|i| Command::ALL.get(i))
            else {
                return;
            };
            let app = NSApplication::sharedApplication(mtm);
            let now = app.currentEvent();
            let key = now.filter(|e| e.r#type() == NSEventType::KeyDown).and_then(|e| chord(&e));
            let took = dispatch(key.map_or(Event::Command(command), Event::Key), mtm);
            if let (false, Some(action)) = (took, in_field(command)) {
                // SAFETY: `undo:` and `redo:` take a sender; with no target
                // they go to the first responder that answers them.
                unsafe { app.sendAction_to_from(action, None, Some(item)) };
            }
        }

        /// A command item is on while the newest model says its command
        /// can act.
        #[unsafe(method(validateMenuItem:))]
        fn validate_menu_item(&self, item: &NSMenuItem) -> bool {
            let command = usize::try_from(item.tag()).ok().and_then(|i| Command::ALL.get(i));
            command.is_none_or(|&(command, _)| {
                MADE.with_borrow(|made| enabled(made.as_ref().map(|(menus, _)| menus), command))
            })
        }
    }
);

thread_local! {
    /// The menus the bar was last made from, and the target of their
    /// command items, which the items do not keep.
    static MADE: RefCell<Option<(Vec<Menu>, Retained<Target>)>> = const { RefCell::new(None) };
}

/// Makes the app's menu bar show `menus`, unless it does already. No menus
/// leave the bar as it is.
pub(super) fn show(menus: &[Menu], mtm: MainThreadMarker) {
    if menus.is_empty() {
        return;
    }
    let kept = MADE.with_borrow_mut(|made| match made {
        Some((m, _)) if same_layout(m, menus) => {
            if m != menus {
                menus.clone_into(m);
            }
            true
        }
        _ => false,
    });
    if kept {
        return;
    }
    // SAFETY: `NSObject`'s `init` on a fresh allocation.
    let target: Retained<Target> = unsafe { msg_send![Target::alloc(mtm), init] };
    let bar = NSMenu::new(mtm);
    for menu in menus {
        let title = NSString::from_str(menu.title);
        let sub = NSMenu::initWithTitle(NSMenu::alloc(mtm), &title);
        for item in &menu.items {
            sub.addItem(&made(item, &target, mtm));
        }
        let head = NSMenuItem::new(mtm);
        head.setTitle(&title);
        head.setSubmenu(Some(&sub));
        bar.addItem(&head);
    }
    NSApplication::sharedApplication(mtm).setMainMenu(Some(&bar));
    MADE.set(Some((menus.to_vec(), target)));
}

/// Whether `command`'s item in `menus` is on; an item not there is.
fn enabled(menus: Option<&Vec<Menu>>, command: Command) -> bool {
    let mut items = menus.into_iter().flatten().flat_map(|m| &m.items);
    items.find(|i| i.action == Action::Run(command)).is_none_or(|i| i.enabled)
}

/// The menu item for `item`: a command's goes to `target`, a native one
/// along the responder chain.
fn made(item: &Item, target: &Target, mtm: MainThreadMarker) -> Retained<NSMenuItem> {
    let key = item.key.map(|c| String::from(c.key())).unwrap_or_default();
    let action = match item.action {
        Action::Run(_) => sel!(run:),
        Action::Native(native) => selector(native),
    };
    // SAFETY: a fresh allocation; every action is a one-argument selector
    // that its target (below, or the responder chain) takes.
    let made = unsafe {
        NSMenuItem::initWithTitle_action_keyEquivalent(
            NSMenuItem::alloc(mtm),
            &NSString::from_str(item.title),
            Some(action),
            &NSString::from_str(&key),
        )
    };
    // The chord's shift, control, option and command bits are `AppKit`'s 17 to 20.
    let mods = item.key.map_or(0, |c| usize::from(c.mods()) << 17);
    made.setKeyEquivalentModifierMask(NSEventModifierFlags(mods));
    if let Action::Run(command) = item.action {
        // SAFETY: `Target` answers `run:`; `MADE` keeps it as long as the item.
        unsafe { made.setTarget(Some(target)) };
        made.setTag(command as isize);
    }
    if item.checked {
        made.setState(NSControlStateValueOn);
    }
    made
}

/// What `command`'s key does in a text field (or a page) that has it: undo
/// and redo its typing, select all its text.
fn in_field(command: Command) -> Option<Sel> {
    match command {
        Command::Undo => Some(sel!(undo:)),
        Command::Redo => Some(sel!(redo:)),
        Command::SelectAll => Some(sel!(selectAll:)),
        _ => None,
    }
}

/// The selector macOS answers for `native`.
fn selector(native: Native) -> Sel {
    match native {
        Native::About => sel!(orderFrontStandardAboutPanel:),
        Native::Cut => sel!(cut:),
        Native::Copy => sel!(copy:),
        Native::Paste => sel!(paste:),
        Native::Minimise => sel!(performMiniaturize:),
    }
}
