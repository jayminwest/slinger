//! The command registry (SPEC D21): each command's title and group, and a
//! chord printed as macOS prints it. The key overlay, the command palette and
//! the menu bar read these and the live [`Keymap`](super::Keymap), so a
//! command added to [`Command::ALL`] shows in all three. Pure.

use super::{ALT, CMD, CTRL, Chord, Command, DOWN, F1, LEFT, RIGHT, SHIFT, UP};

/// Where a command shows: a heading in the key overlay, a menu in the menu bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Group {
    Tools,
    PopUps,
    Sling,
    File,
    Canvas,
    Pages,
    Panes,
    Window,
}

impl Group {
    /// Every group, in the order the surfaces show them.
    pub const ALL: [Group; 8] = [
        Group::Tools,
        Group::PopUps,
        Group::Sling,
        Group::File,
        Group::Canvas,
        Group::Pages,
        Group::Panes,
        Group::Window,
    ];

    /// The group's heading: "Pop-ups".
    pub const fn title(self) -> &'static str {
        match self {
            Group::Tools => "Tools",
            Group::PopUps => "Pop-ups",
            Group::Sling => "Sling",
            Group::File => "File",
            Group::Canvas => "Canvas",
            Group::Pages => "Pages",
            Group::Panes => "Panes",
            Group::Window => "Window",
        }
    }
}

impl Command {
    /// The command's name for people: "Split right".
    pub const fn title(self) -> &'static str {
        match self {
            Command::Quit => "Quit",
            Command::ReloadConfig => "Reload config",
            Command::PickColour => "Pick a colour",
            Command::PickShape => "Pick a shape",
            Command::PickTool => "Pick a tool",
            Command::PickWidth => "Pick a width",
            Command::SplitRight => "Split right",
            Command::SplitDown => "Split down",
            Command::ClosePane => "Close pane",
            Command::FocusNext => "Focus next pane",
            Command::FocusLeft => "Focus pane left",
            Command::FocusRight => "Focus pane right",
            Command::FocusUp => "Focus pane up",
            Command::FocusDown => "Focus pane down",
            Command::GrowPane => "Grow pane",
            Command::ShrinkPane => "Shrink pane",
            Command::ToggleSidebar => "Show or hide sidebar",
            Command::DismissNotice => "Dismiss notice",
            Command::PanLeft => "Pan left",
            Command::PanRight => "Pan right",
            Command::PanUp => "Pan up",
            Command::PanDown => "Pan down",
            Command::ZoomIn => "Zoom in",
            Command::ZoomOut => "Zoom out",
            Command::ZoomReset => "Zoom to 100%",
            Command::NextPage => "Next page",
            Command::DeleteSelection => "Delete selection",
            Command::ClearSelection => "Select nothing",
            Command::ToolPen => "Pen",
            Command::ToolEraser => "Eraser",
            Command::ToolLasso => "Lasso",
            Command::ToolText => "Text",
            Command::ToolBox => "Box",
            Command::ToolArrow => "Arrow",
            Command::ToolSelect => "Select",
            Command::WidthUp => "Thicker pen",
            Command::WidthDown => "Thinner pen",
            Command::Sling => "Sling selection",
            Command::SlingNow => "Sling now",
            Command::PreviousPage => "Previous page",
            Command::NewPage => "New page",
            Command::RenamePage => "Rename page",
            Command::RemovePage => "Remove page",
            Command::Undo => "Undo",
            Command::Redo => "Redo",
            Command::Dictate => "Dictate prompt",
            Command::ShowKeys => "Show keys",
            Command::ShowCommands => "Show commands",
            Command::LeaveFeedback => "Leave feedback…",
            Command::ToggleGrid => "Dotted grid",
            Command::Save => "Save",
            Command::ShowCanvasFile => "Show canvas file in Finder",
            Command::OpenPdf => "Open PDF or tldraw…",
            Command::CropPdf => "Crop PDF",
            Command::OpenWorkspace => "Open workspace…",
            Command::CloseWorkspace => "Close workspace",
            Command::ShowWorkspaceFolder => "Show workspace folder in Finder",
            Command::StartAgent => "Start agent",
            Command::StopAgent => "Stop agent",
            Command::RestartAgent => "Restart agent in a new conversation",
        }
    }

    /// The group the command shows under.
    pub const fn group(self) -> Group {
        match self {
            Command::ToolPen
            | Command::ToolEraser
            | Command::ToolLasso
            | Command::ToolText
            | Command::ToolBox
            | Command::ToolArrow
            | Command::ToolSelect
            | Command::CropPdf
            | Command::WidthUp
            | Command::WidthDown => Group::Tools,
            Command::PickColour | Command::PickShape | Command::PickTool | Command::PickWidth => {
                Group::PopUps
            }
            Command::Sling
            | Command::SlingNow
            | Command::Dictate
            | Command::StartAgent
            | Command::StopAgent
            | Command::RestartAgent => Group::Sling,
            Command::PanLeft
            | Command::PanRight
            | Command::PanUp
            | Command::PanDown
            | Command::ZoomIn
            | Command::ZoomOut
            | Command::ZoomReset
            | Command::DeleteSelection
            | Command::ClearSelection
            | Command::Undo
            | Command::Redo
            | Command::ToggleGrid
            | Command::Save
            | Command::ShowCanvasFile => Group::Canvas,
            Command::OpenPdf
            | Command::OpenWorkspace
            | Command::CloseWorkspace
            | Command::ShowWorkspaceFolder => Group::File,
            Command::NextPage
            | Command::PreviousPage
            | Command::NewPage
            | Command::RenamePage
            | Command::RemovePage => Group::Pages,
            Command::SplitRight
            | Command::SplitDown
            | Command::ClosePane
            | Command::FocusNext
            | Command::FocusLeft
            | Command::FocusRight
            | Command::FocusUp
            | Command::FocusDown
            | Command::GrowPane
            | Command::ShrinkPane => Group::Panes,
            Command::Quit
            | Command::ReloadConfig
            | Command::ToggleSidebar
            | Command::DismissNotice
            | Command::ShowKeys
            | Command::ShowCommands
            | Command::LeaveFeedback => Group::Window,
        }
    }
}

/// Each modifier's glyph, in the order macOS prints them.
pub(super) const MOD_GLYPHS: [(u8, char); 4] = [(CTRL, '⌃'), (ALT, '⌥'), (SHIFT, '⇧'), (CMD, '⌘')];

/// The glyph of each key that does not print as itself.
pub(super) const KEY_GLYPHS: [(char, &str); 10] = [
    (' ', "Space"),
    ('\t', "⇥"),
    ('\r', "↩"),
    ('\u{1b}', "⎋"),
    ('\u{7f}', "⌫"),
    ('\u{f728}', "⌦"),
    (UP, "↑"),
    (DOWN, "↓"),
    (LEFT, "←"),
    (RIGHT, "→"),
];

impl Chord {
    /// The chord as macOS prints it: `cmd+shift+d` is "⇧⌘D", `cmd+return`
    /// is "⌘↩", `f5` is "F5".
    pub fn glyphs(self) -> String {
        let mut out: String =
            MOD_GLYPHS.iter().filter(|&&(bit, _)| self.mods & bit != 0).map(|&(_, g)| g).collect();
        if let Some(&(_, glyph)) = KEY_GLYPHS.iter().find(|&&(c, _)| c == self.key) {
            out.push_str(glyph);
        } else if let Some(n) = function_key(self.key) {
            out.push('F');
            // By hand, as `key_named` parses it: formatting costs the size budget.
            if n >= 10 {
                out.push(char::from(b'0' + n / 10));
            }
            out.push(char::from(b'0' + n % 10));
        } else {
            out.push(self.key.to_ascii_uppercase());
        }
        out
    }
}

/// `n` for the key `fn`, 1 to 20.
fn function_key(key: char) -> Option<u8> {
    let n = u32::from(key).checked_sub(F1)?;
    u8::try_from(n + 1).ok().filter(|&n| n <= 20)
}
