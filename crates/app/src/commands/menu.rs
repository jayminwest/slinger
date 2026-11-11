//! The menu bar (SPEC D21): which menus there are, what each holds, and the
//! key each item shows. `platform` only makes the bar this says. Pure.
//!
//! One dispatch path: a command's key runs through the keymap, wherever
//! `AppKit` sends it, so a menu item never runs a command a second time. A
//! key that types (no command or control held) stays off the menu, as the
//! menu would take it from a text field first. An edit item (cut, copy,
//! paste) goes to the text field with the focus, and keeps its usual key only
//! while the keymap does not bind that chord.

use super::{CMD, Chord, Command, Keymap};
use Action::{Native as N, Run as R};

/// One menu of the bar: "Edit".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Menu {
    pub title: &'static str,
    pub items: Vec<Item>,
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
}

/// What an item does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Run(Command),
    /// What macOS does itself, in the window or the text field with the focus.
    Native(Native),
}

/// The items macOS runs: the about panel, the text field's edits and the
/// window's minimise.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Native {
    About,
    Cut,
    Copy,
    Paste,
    SelectAll,
    Minimise,
}

impl Native {
    const fn title(self) -> &'static str {
        match self {
            Native::About => "About slinger",
            Native::Cut => "Cut",
            Native::Copy => "Copy",
            Native::Paste => "Paste",
            Native::SelectAll => "Select all",
            Native::Minimise => "Minimise",
        }
    }

    /// The usual macOS key: `cmd+x` for cut.
    const fn key(self) -> Option<Chord> {
        let key = match self {
            Native::About => return None,
            Native::Cut => 'x',
            Native::Copy => 'c',
            Native::Paste => 'v',
            Native::SelectAll => 'a',
            Native::Minimise => 'm',
        };
        Some(Chord::new(CMD, key))
    }
}

/// Each menu and its items, in order. The first is the app's own menu.
const BAR: [(&str, &[Action]); 8] = [
    ("slinger", &[N(Native::About), R(Command::ReloadConfig), R(Command::Quit)]),
    (
        "File",
        &[
            R(Command::NewPage),
            R(Command::RenamePage),
            R(Command::RemovePage),
            R(Command::NextPage),
            R(Command::PreviousPage),
        ],
    ),
    (
        "Edit",
        &[
            R(Command::Undo),
            R(Command::Redo),
            N(Native::Cut),
            N(Native::Copy),
            N(Native::Paste),
            N(Native::SelectAll),
            R(Command::DeleteSelection),
            R(Command::ClearSelection),
        ],
    ),
    (
        "Tools",
        &[
            R(Command::ToolPen),
            R(Command::ToolEraser),
            R(Command::ToolLasso),
            R(Command::ToolText),
            R(Command::ToolBox),
            R(Command::ToolArrow),
            R(Command::WidthUp),
            R(Command::WidthDown),
            R(Command::PickColour),
            R(Command::PickShape),
            R(Command::PickTool),
            R(Command::PickWidth),
            R(Command::Sling),
            R(Command::Dictate),
            R(Command::PlaceButton),
        ],
    ),
    (
        "Canvas",
        &[
            R(Command::ZoomIn),
            R(Command::ZoomOut),
            R(Command::PanLeft),
            R(Command::PanRight),
            R(Command::PanUp),
            R(Command::PanDown),
        ],
    ),
    (
        "Pane",
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
    ("Window", &[R(Command::ToggleSidebar), R(Command::DismissNotice), N(Native::Minimise)]),
    ("Help", &[R(Command::ShowKeys), R(Command::ShowCommands)]),
];

/// The menu bar for the keys in `keys`, with a check mark on `checked` (the
/// command of the pen's tool).
pub fn bar(keys: &Keymap, checked: Command) -> Vec<Menu> {
    let item = |action| match action {
        Action::Run(command) => Item {
            title: command.title(),
            action,
            key: keys.chord(command).filter(|c| !c.types()),
            checked: command == checked,
        },
        Action::Native(native) => Item {
            title: native.title(),
            action,
            key: native.key().filter(|&c| keys.command(c).is_none()),
            checked: false,
        },
    };
    BAR.iter()
        .map(|&(title, actions)| Menu { title, items: actions.iter().copied().map(item).collect() })
        .collect()
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
