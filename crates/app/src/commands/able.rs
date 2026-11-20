//! Whether a command can act now ([`can`]): the one enabled predicate. The
//! menu bar greys an item it says no to, and the context menus (plan
//! pl-9880f1) will too. Pure: [`Facts`] holds what the rules read, and
//! `window::facts` reads them from the window, from the same queries the
//! commands' own gates ask (`window::items`, `Canvases::shows_selection`,
//! `Canvases::step_target`, the sling's target, `unslingable`).
//!
//! The rule: false only where running the command now would do nothing,
//! or only flash why it cannot; when unsure, true. A greyed item's key
//! still runs the command, which answers as before (`commands::menu`).

use super::Command;
use crate::scene::daemon::launching;

/// What the window can act on now: its panes, the focused pane, the
/// selection there, the workspace it shows and the daemon the app manages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[expect(clippy::struct_excessive_bools, reason = "one plain fact each, read by `can`")]
pub struct Facts {
    /// How many panes the window shows.
    pub panes: usize,
    /// The focused pane shows a canvas.
    pub canvas: bool,
    /// How many pages the focused pane's canvas has: 0 until its page arrives.
    pub pages: usize,
    /// The focused pane shows an agent UI that has arrived.
    pub ui: bool,
    /// The items hold the keyboard: the window has it and no text of the
    /// app takes typing (`window::items`).
    pub items: bool,
    /// The focused pane shows the selection's page.
    pub selected: bool,
    /// The selection there holds a box to step into or fold.
    pub boxed: bool,
    /// The focused pane is stepped into a box.
    pub inside: bool,
    /// `sling` has something to open its entry at: the selection, a
    /// button, the box the pane is in.
    pub sling: bool,
    /// An entry is open (the sling's, the feedback's, a page's new name's).
    pub entry: bool,
    /// `sling-now` would send: the open entry, or a target with an agent
    /// to sling to.
    pub sling_now: bool,
    /// `new-text` has a canvas page to start a text on.
    pub new_text: bool,
    /// A workspace shows.
    pub workspace: bool,
    /// Its agent, and whether it runs; `None` without one.
    pub agent: Option<bool>,
    /// The notice, or the offer to restart slingerd, shows.
    pub notice: bool,
    /// The app manages a bundled slingerd.
    pub managed: bool,
    /// The daemon that answers predates `daemon_stop`.
    pub old: bool,
}

/// Whether `command` can act on what `facts` says. Every command decides.
pub fn can(command: Command, facts: &Facts) -> bool {
    let f = facts;
    match command {
        Command::Quit
        | Command::ReloadConfig
        | Command::PickColour
        | Command::PickShape
        | Command::PickTool
        | Command::PickWidth
        | Command::PickSize
        | Command::SplitRight
        | Command::SplitDown
        | Command::ToggleSidebar
        | Command::ToolPen
        | Command::ToolEraser
        | Command::ToolLasso
        | Command::ToolText
        | Command::ToolBox
        | Command::ToolArrow
        | Command::ToolSelect
        | Command::CropPdf
        | Command::WidthUp
        | Command::WidthDown
        // With nothing to resize they set the next text's size.
        | Command::SizeUp
        | Command::SizeDown
        // The app does not know the daemon's undo stack.
        | Command::Undo
        | Command::Redo
        | Command::ShowKeys
        | Command::ShowCommands
        | Command::LeaveFeedback
        | Command::ShowAllFeedback
        | Command::ShowFeedbackFile
        | Command::ToggleGrid
        | Command::OpenPdf
        | Command::OpenWorkspace
        // A text field's or a page's own select all, where the items do not have the keys.
        | Command::SelectAll => true,
        Command::ClosePane
        | Command::FocusNext
        | Command::FocusLeft
        | Command::FocusRight
        | Command::FocusUp
        | Command::FocusDown
        | Command::GrowPane
        | Command::ShrinkPane => f.panes > 1,
        Command::DismissNotice => f.notice,
        Command::PanLeft
        | Command::PanRight
        | Command::PanUp
        | Command::PanDown
        | Command::ZoomIn
        | Command::ZoomOut
        | Command::ZoomReset
        | Command::ZoomToFit
        | Command::Save
        | Command::ShowCanvasFile
        // With no find yet, find-next asks as find does.
        | Command::Find
        | Command::FindNext => f.canvas,
        Command::NewPage | Command::RenamePage => f.pages > 0,
        Command::NextPage | Command::PreviousPage | Command::RemovePage => f.pages > 1,
        Command::NextItem | Command::PreviousItem => f.items && f.pages > 0,
        Command::DeleteSelection
        | Command::Duplicate
        | Command::NudgeLeft
        | Command::NudgeRight
        | Command::NudgeUp
        | Command::NudgeDown
        | Command::NudgeLeftFine
        | Command::NudgeRightFine
        | Command::NudgeUpFine
        | Command::NudgeDownFine => f.items && f.selected,
        // With nothing selected it steps out.
        Command::ClearSelection => f.items && (f.selected || f.inside),
        Command::Fold => f.items && f.boxed,
        Command::StepIn | Command::StepInBeside => f.boxed,
        Command::StepOut => f.inside,
        Command::Sling => f.sling,
        // With an entry open it speaks into it.
        Command::Dictate => f.entry || f.sling,
        Command::SlingNow => f.sling_now,
        Command::NewText => f.new_text,
        Command::CloseWorkspace | Command::ShowWorkspaceFolder => f.workspace,
        Command::StartAgent => f.workspace && f.agent == Some(false),
        Command::StopAgent => f.workspace && f.agent == Some(true),
        Command::RestartAgent => f.workspace && f.agent.is_some(),
        Command::ExportUi => f.ui,
        Command::StartSlingerd | Command::RestartDaemon => {
            launching(command, f.managed, f.old).is_ok()
        }
    }
}

#[cfg(test)]
mod tests;
