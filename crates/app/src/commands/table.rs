//! The command table: each command's name in the config file, its title,
//! its group and its default chord, in one row. [`Command::ALL`],
//! [`Command::named`], [`Command::title`], [`Command::group`] and
//! [`Keymap::default`](super::Keymap) read it, so a new command is a new
//! variant and a new row here. The rows are in the enum's order (the
//! surfaces list commands in it), which the build checks. Pure.

use super::Group::{Canvas, File, Pages, Panes, PopUps, Sling, Tools, Window};
use super::{ALT, CMD, CTRL, Chord, Command as C, DOWN, Group, LEFT, RIGHT, SHIFT, UP};

/// One command's facts.
pub(super) struct Row {
    pub command: C,
    /// Its name in the config file: "split-right".
    pub name: &'static str,
    /// Its name for people: "Split right".
    pub title: &'static str,
    /// Where it shows: a heading in the key overlay, a menu in the menu bar.
    pub group: Group,
    /// The chord that runs it unless the config says otherwise. A command
    /// with none runs from the palette and the menu bar until the user
    /// binds one. `config.example.toml` lists each.
    pub key: Option<Chord>,
}

/// A row whose default chord is `mods` and `key`, as the key event gives them.
const fn row(
    command: C,
    name: &'static str,
    title: &'static str,
    group: Group,
    mods: u8,
    key: char,
) -> Row {
    Row { command, name, title, group, key: Some(Chord { mods, key }) }
}

/// A row with no default chord.
const fn bare(command: C, name: &'static str, title: &'static str, group: Group) -> Row {
    Row { command, name, title, group, key: None }
}

/// Every command, in the enum's order.
pub(super) const TABLE: &[Row] = &[
    row(C::Quit, "quit", "Quit slinger", Window, CMD, 'q'),
    row(C::ReloadConfig, "reload-config", "Reload config", Window, CMD, 'r'),
    row(C::PickColour, "pick-colour", "Pick a colour", PopUps, CMD | SHIFT, 'c'),
    row(C::PickShape, "pick-shape", "Pick a shape", PopUps, CMD | SHIFT, 's'),
    row(C::PickTool, "pick-tool", "Pick a tool", PopUps, CMD | SHIFT, 't'),
    row(C::PickWidth, "pick-width", "Pick a width", PopUps, CMD | SHIFT, 'w'),
    row(C::SplitRight, "split-right", "Split right", Panes, CMD, 'd'),
    row(C::SplitDown, "split-down", "Split down", Panes, CMD | SHIFT, 'd'),
    row(C::ClosePane, "close-pane", "Close pane", Panes, CMD, 'w'),
    row(C::FocusNext, "focus-next", "Focus next pane", Panes, CTRL, '\t'),
    row(C::FocusLeft, "focus-left", "Focus pane left", Panes, CMD | ALT, LEFT),
    row(C::FocusRight, "focus-right", "Focus pane right", Panes, CMD | ALT, RIGHT),
    row(C::FocusUp, "focus-up", "Focus pane up", Panes, CMD | ALT, UP),
    row(C::FocusDown, "focus-down", "Focus pane down", Panes, CMD | ALT, DOWN),
    row(C::GrowPane, "grow-pane", "Grow pane", Panes, ALT, '='),
    row(C::ShrinkPane, "shrink-pane", "Shrink pane", Panes, ALT, '-'),
    row(C::ToggleSidebar, "toggle-sidebar", "Show or hide sidebar", Window, CMD, 'b'),
    row(C::DismissNotice, "dismiss-notice", "Dismiss notice", Window, CMD, '.'),
    row(C::PanLeft, "pan-left", "Pan left", Canvas, ALT, LEFT),
    row(C::PanRight, "pan-right", "Pan right", Canvas, ALT, RIGHT),
    row(C::PanUp, "pan-up", "Pan up", Canvas, ALT, UP),
    row(C::PanDown, "pan-down", "Pan down", Canvas, ALT, DOWN),
    row(C::ZoomIn, "zoom-in", "Zoom in", Canvas, CMD, '='),
    row(C::ZoomOut, "zoom-out", "Zoom out", Canvas, CMD, '-'),
    row(C::ZoomReset, "zoom-reset", "Zoom to 100%", Canvas, CMD, '0'),
    row(C::NextPage, "next-page", "Next page", Pages, CMD, ']'),
    row(C::DeleteSelection, "delete-selection", "Delete selection", Canvas, 0, '\u{7f}'),
    row(C::ClearSelection, "clear-selection", "Select nothing", Canvas, 0, '\u{1b}'),
    row(C::ToolPen, "tool-pen", "Pen", Tools, CMD, '1'),
    row(C::ToolEraser, "tool-eraser", "Eraser", Tools, CMD, '2'),
    row(C::ToolLasso, "tool-lasso", "Lasso", Tools, CMD, '3'),
    row(C::ToolText, "tool-text", "Text", Tools, CMD, '4'),
    row(C::ToolBox, "tool-box", "Box", Tools, CMD, '5'),
    row(C::ToolArrow, "tool-arrow", "Arrow", Tools, CMD, '6'),
    row(C::ToolSelect, "tool-select", "Select", Tools, CMD, '7'),
    row(C::WidthUp, "width-up", "Thicker pen", Tools, ALT, ']'),
    row(C::WidthDown, "width-down", "Thinner pen", Tools, ALT, '['),
    row(C::Sling, "sling", "Sling selection", Sling, CMD, '\r'),
    row(C::PreviousPage, "previous-page", "Previous page", Pages, CMD, '['),
    row(C::NewPage, "new-page", "New page", Pages, CMD, 'n'),
    row(C::RenamePage, "rename-page", "Rename page", Pages, CMD | SHIFT, 'r'),
    row(C::RemovePage, "remove-page", "Remove page", Pages, CMD | SHIFT, '\u{7f}'),
    row(C::Undo, "undo", "Undo", Canvas, CMD, 'z'),
    row(C::Redo, "redo", "Redo", Canvas, CMD | SHIFT, 'z'),
    row(C::Dictate, "dictate", "Dictate prompt", Sling, CMD | SHIFT, '\r'),
    row(C::ShowKeys, "show-keys", "Show keys", Window, CMD, '/'),
    row(C::ShowCommands, "show-commands", "Show commands", Window, CMD, 'k'),
    row(C::LeaveFeedback, "leave-feedback", "Leave feedback…", Window, CMD | SHIFT, 'f'),
    row(C::ToggleGrid, "toggle-grid", "Dotted grid", Canvas, CMD, '\''),
    row(C::SlingNow, "sling-now", "Sling now", Sling, ALT, '\r'),
    row(C::Save, "save", "Save", Canvas, CMD, 's'),
    row(
        C::ShowCanvasFile,
        "show-canvas-file",
        "Show canvas file in Finder",
        Canvas,
        CMD | ALT,
        'r',
    ),
    row(C::OpenPdf, "open-pdf", "Open PDF or tldraw…", File, CMD, 'o'),
    row(C::CropPdf, "crop-pdf", "Crop PDF", Tools, CMD, '8'),
    row(C::OpenWorkspace, "open-workspace", "Open workspace…", File, CMD | SHIFT, 'o'),
    row(C::CloseWorkspace, "close-workspace", "Close workspace", File, CMD | ALT, 'w'),
    bare(C::ShowWorkspaceFolder, "show-workspace-folder", "Show workspace folder in Finder", File),
    row(C::StartAgent, "start-agent", "Start agent", Sling, CMD | ALT, 'a'),
    row(C::StopAgent, "stop-agent", "Stop agent", Sling, CMD | ALT, '.'),
    row(
        C::RestartAgent,
        "restart-agent",
        "Restart agent in a new conversation",
        Sling,
        CMD | ALT | SHIFT,
        'a',
    ),
    bare(C::ExportUi, "export-ui", "Export agent UI…", File),
    bare(C::ShowFeedbackFile, "show-feedback-file", "Show feedback file in Finder", Window),
    bare(C::ShowAllFeedback, "show-all-feedback", "Show all feedback…", Window),
    row(C::PickSize, "pick-size", "Pick a text size", PopUps, CMD | SHIFT, 'a'),
    row(C::SizeUp, "size-up", "Bigger text", Tools, ALT | SHIFT, '}'),
    row(C::SizeDown, "size-down", "Smaller text", Tools, ALT | SHIFT, '{'),
    row(C::SelectAll, "select-all", "Select all", Canvas, CMD, 'a'),
    row(C::NextItem, "next-item", "Select next item", Canvas, 0, '\t'),
    row(C::PreviousItem, "previous-item", "Select previous item", Canvas, SHIFT, '\t'),
    row(C::NudgeLeft, "nudge-left", "Nudge left", Canvas, 0, LEFT),
    row(C::NudgeRight, "nudge-right", "Nudge right", Canvas, 0, RIGHT),
    row(C::NudgeUp, "nudge-up", "Nudge up", Canvas, 0, UP),
    row(C::NudgeDown, "nudge-down", "Nudge down", Canvas, 0, DOWN),
    row(C::Duplicate, "duplicate", "Duplicate", Canvas, CMD, 'j'),
    row(C::Find, "find", "Find text…", Canvas, CMD, 'f'),
    row(C::FindNext, "find-next", "Find next", Canvas, CMD, 'g'),
    row(C::NewText, "new-text", "New text", Canvas, CTRL, '\r'),
    row(C::Fold, "fold", "Fold or unfold box", Canvas, CMD | ALT, 'f'),
    row(C::NudgeLeftFine, "nudge-left-fine", "Nudge left a point", Canvas, SHIFT, LEFT),
    row(C::NudgeRightFine, "nudge-right-fine", "Nudge right a point", Canvas, SHIFT, RIGHT),
    row(C::NudgeUpFine, "nudge-up-fine", "Nudge up a point", Canvas, SHIFT, UP),
    row(C::NudgeDownFine, "nudge-down-fine", "Nudge down a point", Canvas, SHIFT, DOWN),
    row(C::StepIn, "step-in", "Step into box", Canvas, CMD | ALT, 'i'),
    row(C::StepInBeside, "step-in-beside", "Step into box beside", Canvas, CMD | ALT | SHIFT, 'i'),
    row(C::StepOut, "step-out", "Step out of box", Canvas, CMD | ALT, 'o'),
    bare(C::StartSlingerd, "start-slingerd", "Start slingerd", Window),
    bare(C::RestartDaemon, "restart-daemon", "Restart slingerd", Window),
    row(C::ZoomToFit, "zoom-to-fit", "Zoom to fit", Canvas, CMD, '9'),
    bare(C::ReloadUi, "reload-ui", "Reload agent UI", File),
    row(C::UnlockFigure, "unlock-figure", "Unlock figure", Canvas, CMD | ALT, 'u'),
];

// Each row is at its command's index: `Command as usize` finds its row.
const _: () = {
    let mut i = 0;
    while i < TABLE.len() {
        assert!(TABLE[i].command as usize == i, "the table is in the enum's order");
        i += 1;
    }
};
