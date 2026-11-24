//! The context menus (plan pl-9880f1): what a context click on a thing
//! offers ([`items`]). The thing is named in the type ([`Subject`]). Each
//! item is the bar's ([`menu::item`]): the command's title, its live key,
//! grey where [`able::can`](super::able::can) says it cannot act. A pick
//! runs the command as its menu bar item does, on what the click made
//! current (the pane it focused, the pointer it moved, the item it
//! selected: Finder's rule, `Canvases::context_at`). A sidebar workspace
//! row and a divider cannot be made current (a switch is slow, and a
//! divider has no focus), so a pick carries its popup's subject (the
//! window's `Event::Picked`), and the app keeps no memory of the last
//! popup. `platform` only pops the menu a [`Popup`] says. Pure.

use std::path::PathBuf;

use super::menu::{self, Action, Item, Native};
use super::{Command, Keymap, able::Facts};
use crate::layout::{PaneId, Point};
use Action::{Native as N, Run as R, Separator as S};

/// What a context click was on.
#[derive(Debug, Clone, PartialEq)]
pub enum Subject {
    /// The empty part of a canvas pane.
    Canvas,
    /// A locked item, which no click selects: the canvas's menu, with
    /// `unlock-figure` first (docs/design/figures.md, "The unlock").
    Figure,
    /// A pane's footer; `ui` when the pane shows an agent UI.
    Pane { ui: bool },
    /// One item, selected alone (Finder's rule), of this kind.
    Item { kind: Kind },
    /// A selection of more than one item; `boxed` when it holds a box.
    Selection { boxed: bool },
    /// A canvas text being typed: its own cut, copy and paste, and
    /// write focus on it.
    Typing,
    /// A workspace's rows in the sidebar, by its folder (what closes it,
    /// whatever its file holds), never by its row, which can move.
    Workspace { dir: PathBuf },
    /// The divider after pane `before` (the server's stable id), its
    /// split's share now.
    Divider { before: PaneId, ratio: f64 },
    /// The rest of the open sidebar.
    Sidebar,
}

/// The kind of item a context click was on: what its menu adds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Stroke,
    Text,
    Box,
    Arrow,
    /// A PDF page the select tool takes (a snippet, or a page on a
    /// canvas that is not the PDF's own).
    Pdf,
    /// A live item, its page not taking presses (not come, or too small).
    Live,
    /// A canvas text with more rows than it shows ("N more lines"): a
    /// text's items, and stepping into it.
    LongText,
}

/// A context menu to pop: where (in the window, from its top left) and
/// what it holds.
#[derive(Debug, Clone, PartialEq)]
pub struct Popup {
    pub at: Point,
    pub items: Vec<Item>,
    /// What it is for: a pick carries it.
    pub subject: Subject,
}

/// The items `subject`'s menu holds, in order: titles and keys as `keys`
/// has them, a check on `checked`, each grey where `facts` say its command
/// cannot act. No menu starts or ends with a separator or has two in a row.
pub fn items(subject: &Subject, keys: &Keymap, checked: Command, facts: &Facts) -> Vec<Item> {
    let actions = actions(subject);
    tidy(actions.iter().map(|&action| menu::item(action, keys, checked, facts)).collect())
}

/// Whether a pick of `command` from `subject`'s menu acts on that subject
/// rather than on what is current: a workspace row's close, show folder
/// and agent commands act on its workspace, a divider's grow and shrink
/// on it. Any other pick runs as its menu bar item does.
pub fn carried(command: Command, subject: &Subject) -> bool {
    match subject {
        Subject::Workspace { .. } => matches!(
            command,
            Command::CloseWorkspace
                | Command::ShowWorkspaceFolder
                | Command::StartAgent
                | Command::StopAgent
                | Command::RestartAgent
        ),
        Subject::Divider { .. } => matches!(command, Command::GrowPane | Command::ShrinkPane),
        _ => false,
    }
}

/// What any item, or a selection, offers first: sling it, the clipboard's
/// cut and copy, duplicate and delete it.
const ANY: [Action; 7] = [
    R(Command::Sling),
    S,
    N(Native::Cut),
    N(Native::Copy),
    R(Command::Duplicate),
    R(Command::DeleteSelection),
    S,
];

/// `subject`'s actions, separators where its groups meet.
fn actions(subject: &Subject) -> Vec<Action> {
    let extra: &[Action] = match *subject {
        Subject::Item { kind: Kind::Live } => {
            return vec![R(Command::Sling), R(Command::DeleteSelection)];
        }
        Subject::Typing => {
            return vec![
                N(Native::Cut),
                N(Native::Copy),
                N(Native::Paste),
                S,
                R(Command::WriteFocus),
            ];
        }
        Subject::Item { kind: Kind::Stroke | Kind::Arrow } => &[],
        Subject::Item { kind: Kind::Text } => &[
            R(Command::PickSize),
            R(Command::SizeUp),
            R(Command::SizeDown),
            S,
            R(Command::WriteFocus),
        ],
        Subject::Item { kind: Kind::LongText } => &[
            R(Command::PickSize),
            R(Command::SizeUp),
            R(Command::SizeDown),
            S,
            R(Command::WriteFocus),
            S,
            R(Command::StepIn),
            R(Command::StepInBeside),
        ],
        Subject::Item { kind: Kind::Box } => {
            &[S, R(Command::Fold), R(Command::StepIn), R(Command::StepInBeside)]
        }
        Subject::Item { kind: Kind::Pdf } => &[S, R(Command::CropPdf), R(Command::ShowCanvasFile)],
        Subject::Selection { boxed } => {
            let fold = boxed.then_some(R(Command::Fold));
            let tail = [R(Command::PickColour), R(Command::ClearSelection)];
            return ANY.into_iter().chain(fold).chain(tail).collect();
        }
        Subject::Workspace { .. } => {
            return vec![
                R(Command::CloseWorkspace),
                R(Command::ShowWorkspaceFolder),
                S,
                R(Command::StartAgent),
                R(Command::StopAgent),
                R(Command::RestartAgent),
            ];
        }
        Subject::Divider { .. } => return vec![R(Command::GrowPane), R(Command::ShrinkPane)],
        Subject::Sidebar => return vec![R(Command::OpenWorkspace), R(Command::ToggleSidebar)],
        Subject::Figure => {
            let canvas = panes(&Subject::Canvas).iter().copied();
            return [R(Command::UnlockFigure), S].into_iter().chain(canvas).collect();
        }
        Subject::Canvas | Subject::Pane { .. } => return panes(subject).to_vec(),
    };
    ANY.into_iter().chain([R(Command::PickColour)]).chain(extra.iter().copied()).collect()
}

/// The actions of a pane's footer or the empty canvas.
fn panes(subject: &Subject) -> &'static [Action] {
    match *subject {
        Subject::Canvas => &[
            N(Native::Paste),
            R(Command::NewText),
            R(Command::SelectAll),
            S,
            R(Command::SlingNow),
            S,
            R(Command::ZoomReset),
            R(Command::ToggleGrid),
            S,
            R(Command::NewPage),
            R(Command::StepOut),
        ],
        Subject::Pane { ui: true } => &[
            R(Command::ReloadUi),
            R(Command::ExportUi),
            S,
            R(Command::SplitRight),
            R(Command::SplitDown),
            R(Command::ClosePane),
        ],
        Subject::Pane { ui: false } => {
            &[R(Command::SplitRight), R(Command::SplitDown), R(Command::ClosePane)]
        }
        _ => &[],
    }
}

/// `items` with no separator first, last or after another.
fn tidy(items: Vec<Item>) -> Vec<Item> {
    let line = |i: &Item| i.action == Action::Separator;
    let mut out: Vec<Item> = Vec::with_capacity(items.len());
    for item in items {
        if !line(&item) || out.last().is_some_and(|last| !line(last)) {
            out.push(item);
        }
    }
    if out.last().is_some_and(line) {
        out.pop();
    }
    out
}

#[cfg(test)]
mod tests;
