//! The menu bar (SPEC D21): which menus there are, what each holds, and the
//! key each item shows. `platform` only makes the bar this says. Pure.
//!
//! One dispatch path: a command's key runs through the keymap, wherever
//! `AppKit` sends it, so a menu item never runs a command a second time. A
//! key that types (no command or control held) stays off the menu, as the
//! menu would take it from a text field first. An edit item (cut, copy,
//! paste) goes to the text field with the focus, and keeps its usual key only
//! while the keymap does not bind that chord; so do the items every Mac app
//! has (hide, full screen, zoom...), which macOS runs itself. Each menu has a
//! [`Role`], which tells `AppKit` which menu is the app's, the Window menu
//! and the Help menu.
//!
//! A command's item is grey when [`able::can`] says the command cannot act
//! now. That never changes what its key does: `AppKit` does not fire a
//! disabled item for its key, so the key reaches the keymap as any key
//! does, and the command answers as before (a flash that says why). An
//! edit item is always on here: the window greys cut, copy and paste
//! itself (`Event::Can`). A bar that differs from the last only in which
//! items are grey ([`same_layout`]) is not made again.

use super::able::{self, Facts};
use super::{ALT, CMD, CTRL, Chord, Command, Keymap};
use Action::{Native as N, Run as R, Separator as S};

/// One menu of the bar: "Edit".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Menu {
    pub title: &'static str,
    pub role: Role,
    pub items: Vec<Item>,
}

/// What a menu is to `AppKit`: the app's own (named after it), the Window
/// menu (which lists the windows), the Help menu (which has the search
/// field), or any other.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    App,
    Plain,
    Window,
    Help,
}

/// One item of a menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Item {
    pub title: &'static str,
    pub action: Action,
    /// The key the item shows and answers to, if any.
    pub key: Option<Chord>,
    /// A check mark: the tool the pen has.
    pub checked: bool,
    /// Not grey: its command can act now.
    pub enabled: bool,
}

/// What an item does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Run(Command),
    /// What macOS does itself, in the window or the text field with the focus.
    Native(Native),
    /// A line between groups of items.
    Separator,
}

/// The items macOS runs: the about panel, the Services submenu, hiding the
/// app, the text field's edits, and the window's minimise, zoom and full
/// screen; macOS's own titles, which it localises and changes ("Exit Full
/// Screen").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Native {
    About,
    Services,
    Hide,
    HideOthers,
    ShowAll,
    Cut,
    Copy,
    Paste,
    Minimise,
    Zoom,
    FullScreen,
    BringAllToFront,
}

impl Native {
    const fn title(self) -> &'static str {
        match self {
            Native::About => "About slinger",
            Native::Services => "Services",
            Native::Hide => "Hide slinger",
            Native::HideOthers => "Hide Others",
            Native::ShowAll => "Show All",
            Native::Cut => "Cut",
            Native::Copy => "Copy",
            Native::Paste => "Paste",
            Native::Minimise => "Minimize",
            Native::Zoom => "Zoom",
            Native::FullScreen => "Enter Full Screen",
            Native::BringAllToFront => "Bring All to Front",
        }
    }

    /// The usual macOS key: `cmd+x` for cut.
    pub const fn key(self) -> Option<Chord> {
        let (mods, key) = match self {
            Native::About
            | Native::Services
            | Native::ShowAll
            | Native::Zoom
            | Native::BringAllToFront => return None,
            Native::Hide => (CMD, 'h'),
            Native::HideOthers => (CMD | ALT, 'h'),
            Native::Cut => (CMD, 'x'),
            Native::Copy => (CMD, 'c'),
            Native::Paste => (CMD, 'v'),
            Native::Minimise => (CMD, 'm'),
            Native::FullScreen => (CMD | CTRL, 'f'),
        };
        Some(Chord::new(mods, key))
    }
}

/// Each menu, its role and its items, in order. The first is the app's own menu.
const BAR: [(&str, Role, &[Action]); 8] = [
    (
        "slinger",
        Role::App,
        &[
            N(Native::About),
            S,
            R(Command::ReloadConfig),
            S,
            N(Native::Services),
            S,
            N(Native::Hide),
            N(Native::HideOthers),
            N(Native::ShowAll),
            S,
            R(Command::Quit),
        ],
    ),
    (
        "File",
        Role::Plain,
        &[
            R(Command::OpenWorkspace),
            R(Command::CloseWorkspace),
            R(Command::ShowWorkspaceFolder),
            R(Command::OpenPdf),
            R(Command::Save),
            R(Command::ShowCanvasFile),
            R(Command::ExportUi),
            R(Command::ReloadUi),
            R(Command::NewPage),
            R(Command::RenamePage),
            R(Command::RemovePage),
            R(Command::NextPage),
            R(Command::PreviousPage),
        ],
    ),
    (
        "Edit",
        Role::Plain,
        &[
            R(Command::Undo),
            R(Command::Redo),
            N(Native::Cut),
            N(Native::Copy),
            N(Native::Paste),
            R(Command::NewText),
            R(Command::SelectAll),
            R(Command::DeleteSelection),
            R(Command::ClearSelection),
            R(Command::Duplicate),
            R(Command::UnlockFigure),
            R(Command::Fold),
            R(Command::StepIn),
            R(Command::StepInBeside),
            R(Command::StepOut),
            R(Command::NextItem),
            R(Command::PreviousItem),
            R(Command::NudgeLeft),
            R(Command::NudgeRight),
            R(Command::NudgeUp),
            R(Command::NudgeDown),
            R(Command::NudgeLeftFine),
            R(Command::NudgeRightFine),
            R(Command::NudgeUpFine),
            R(Command::NudgeDownFine),
            R(Command::Find),
            R(Command::FindNext),
        ],
    ),
    (
        "Tools",
        Role::Plain,
        &[
            R(Command::ToolPen),
            R(Command::ToolEraser),
            R(Command::ToolLasso),
            R(Command::ToolText),
            R(Command::ToolBox),
            R(Command::ToolArrow),
            R(Command::ToolSelect),
            R(Command::CropPdf),
            R(Command::WidthUp),
            R(Command::WidthDown),
            R(Command::SizeUp),
            R(Command::SizeDown),
            R(Command::PickColour),
            R(Command::PickShape),
            R(Command::PickTool),
            R(Command::PickWidth),
            R(Command::PickSize),
            R(Command::Sling),
            R(Command::SlingNow),
            R(Command::Dictate),
            R(Command::StartAgent),
            R(Command::StopAgent),
            R(Command::RestartAgent),
        ],
    ),
    (
        "Canvas",
        Role::Plain,
        &[
            R(Command::ZoomIn),
            R(Command::ZoomOut),
            R(Command::ZoomReset),
            R(Command::ZoomToFit),
            R(Command::WriteFocus),
            R(Command::PanLeft),
            R(Command::PanRight),
            R(Command::PanUp),
            R(Command::PanDown),
            R(Command::ToggleGrid),
        ],
    ),
    (
        "Pane",
        Role::Plain,
        &[
            R(Command::SplitRight),
            R(Command::SplitDown),
            R(Command::ClosePane),
            R(Command::FocusNext),
            R(Command::FocusLeft),
            R(Command::FocusRight),
            R(Command::FocusUp),
            R(Command::FocusDown),
            R(Command::GrowPane),
            R(Command::ShrinkPane),
        ],
    ),
    (
        "Window",
        Role::Window,
        &[
            N(Native::Minimise),
            N(Native::Zoom),
            N(Native::FullScreen),
            S,
            R(Command::ToggleSidebar),
            R(Command::DismissNotice),
            R(Command::StartSlingerd),
            R(Command::RestartDaemon),
            S,
            N(Native::BringAllToFront),
        ],
    ),
    (
        "Help",
        Role::Help,
        &[
            R(Command::ShowKeys),
            R(Command::ShowCommands),
            R(Command::LeaveFeedback),
            R(Command::ShowAllFeedback),
            R(Command::ShowFeedbackFile),
        ],
    ),
];

/// The menu bar for the keys in `keys`, with a check mark on `checked` (the
/// command of the pen's tool), each command's item grey where `facts` say
/// it cannot act.
pub fn bar(keys: &Keymap, checked: Command, facts: &Facts) -> Vec<Menu> {
    let menu = |&(title, role, actions): &(_, _, &[Action])| Menu {
        title,
        role,
        items: actions.iter().map(|&action| item(action, keys, checked, facts)).collect(),
    };
    BAR.iter().map(menu).collect()
}

/// The item for `action`, the one way every menu (the bar, a context
/// menu) makes one: a command's title, its key in `keys` unless it types,
/// a check mark on `checked`, grey where `facts` say it cannot act; a
/// native item's macOS title and its usual key while the keymap leaves it
/// free, always on.
pub fn item(action: Action, keys: &Keymap, checked: Command, facts: &Facts) -> Item {
    match action {
        Action::Run(command) => Item {
            title: command.title(),
            action,
            key: keys.chord(command).filter(|c| !c.types()),
            checked: command == checked,
            enabled: able::can(command, facts),
        },
        Action::Native(native) => Item {
            title: native.title(),
            action,
            key: native.key().filter(|&c| keys.command(c).is_none()),
            checked: false,
            enabled: true,
        },
        Action::Separator => Item { title: "", action, key: None, checked: false, enabled: true },
    }
}

/// Whether the bars `a` and `b` differ at most in which items are grey:
/// the same menus, items, titles, keys and check marks, in the same order.
pub fn same_layout(a: &[Menu], b: &[Menu]) -> bool {
    let item = |i: &Item| Item { enabled: true, ..*i };
    let menu = |(a, b): (&Menu, &Menu)| {
        (a.title, a.role) == (b.title, b.role)
            && a.items.len() == b.items.len()
            && a.items.iter().zip(&b.items).all(|(x, y)| item(x) == item(y))
    };
    a.len() == b.len() && a.iter().zip(b).all(menu)
}

impl Chord {
    /// The modifier bits: [`super::SHIFT`], [`super::CTRL`], [`super::ALT`]
    /// and [`CMD`].
    pub const fn mods(self) -> u8 {
        self.mods
    }
}

#[cfg(test)]
mod tests;
