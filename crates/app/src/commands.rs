//! Key commands (SPEC P4): every action the app takes from the keyboard is a
//! named [`Command`], bound to a [`Chord`] in the config file. A tablet button
//! is a key, set in the tablet's own software. Pure: `platform` turns a key
//! event into a chord, and a [`Keymap`] turns the chord into a command.
//! `registry` gives each command a title and a group for the surfaces that
//! list them; `menu` lays them out as the menu bar.

mod chord;
pub mod menu;
mod registry;

use chord::F1;
pub use chord::{DELETE, DOWN, END, FORWARD_DELETE, HOME, LEFT, RIGHT, UP};
pub use registry::Group;

/// Every action a key can run. Later seeds add to [`Command::ALL`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    Quit,
    ReloadConfig,
    /// Open the colour pop-up at the pen.
    PickColour,
    /// Open the shape pop-up (box, arrow) at the pen.
    PickShape,
    /// Open the tool pop-up (pen, eraser, lasso, text, select) at the pen.
    PickTool,
    /// Open the width pop-up (`[pen] widths`) at the pen.
    PickWidth,
    /// Split the focused pane; the new pane, on the right, shows the same.
    SplitRight,
    /// Split the focused pane; the new pane, below, shows the same.
    SplitDown,
    /// Close the focused pane. The last pane stays.
    ClosePane,
    FocusNext,
    FocusLeft,
    FocusRight,
    FocusUp,
    FocusDown,
    /// Give the focused pane more of its split.
    GrowPane,
    ShrinkPane,
    /// Open the sidebar, or collapse it.
    ToggleSidebar,
    /// Hide the notice that an agent is blocked or done.
    DismissNotice,
    /// Move the focused pane's canvas: the view goes left (the ink right).
    PanLeft,
    PanRight,
    PanUp,
    PanDown,
    /// Zoom the focused pane's canvas in or out about the pane's centre.
    ZoomIn,
    ZoomOut,
    /// Zoom the focused pane's canvas to 100% about the pane's centre.
    ZoomReset,
    /// Turn the focused pane to the next page of its canvas, after the last
    /// to the first. Other panes keep their page.
    NextPage,
    /// Remove the selection, in one undo step.
    DeleteSelection,
    /// Select nothing.
    ClearSelection,
    /// Make the pen draw, erase, lasso, place text, draw boxes, draw
    /// arrows or select, without a pop-up.
    ToolPen,
    ToolEraser,
    ToolLasso,
    ToolText,
    ToolBox,
    ToolArrow,
    ToolSelect,
    /// Make the pen's line the next width of `[pen] widths`, or the one
    /// before, without a pop-up. At either end it stays.
    WidthUp,
    WidthDown,
    /// Sling the selection: a one-line prompt opens at it.
    Sling,
    /// Turn the focused pane to the page before, before the first to the last.
    PreviousPage,
    /// Add a page after the last; the focused pane turns to it.
    NewPage,
    /// Type a new name for the focused pane's page.
    RenamePage,
    /// Remove the focused pane's page, asking once when it has items.
    RemovePage,
    /// Undo the user's last step on the focused pane's canvas; an agent's
    /// steps are its own.
    Undo,
    Redo,
    /// Speak the sling's prompt: Flick's dictation starts (opening the
    /// prompt), and the next press stops it.
    Dictate,
    /// Show every command by group with its key (the key overlay).
    ShowKeys,
    /// Find a command by its title and run it (the command palette).
    ShowCommands,
    /// Leave a note about slinger itself: a one-line entry opens in the
    /// focused pane, and the daemon keeps it in its feedback file.
    LeaveFeedback,
    /// Show the dotted grid on every canvas pane, or hide it.
    ToggleGrid,
    /// Sling at once, with no prompt: the selection, or with nothing
    /// selected what is new on the focused pane's page.
    SlingNow,
    /// Say where the focused pane's canvas is saved: the daemon writes
    /// every change as it is made, so there is nothing more to do.
    Save,
    /// Show the focused pane's canvas file in the Finder.
    ShowCanvasFile,
    /// Open a PDF as a new canvas, one page under the next, in a pane
    /// beside the focused one (docs/design/pdf.md); or a tldraw file,
    /// imported as a canvas (`canvas_import`), in a pane there.
    OpenPdf,
    /// Make the pen crop a PDF: a box dragged on a `pdf` item puts that
    /// part of its page on the clipboard (docs/design/pdf.md).
    CropPdf,
    /// Open a folder as a workspace, making its file the first time (the
    /// open panel can make a new folder), and show it in the window.
    OpenWorkspace,
    /// Close the workspace the window shows (its file, canvas and agent
    /// stay), and show the first left.
    CloseWorkspace,
    /// Show the folder of the workspace the window shows in the Finder.
    ShowWorkspaceFolder,
    /// Start the agent of the workspace the window shows, with its own
    /// limits, in its last conversation; one that runs stays as it is.
    StartAgent,
    /// Stop the agent of the workspace the window shows: its process ends,
    /// and its conversation stays for the next start.
    StopAgent,
    /// Start the agent of the workspace the window shows in a new
    /// conversation: a running process ends first, and its last
    /// conversation is dropped.
    RestartAgent,
    /// Save the agent UI the focused pane shows, the version it shows, as
    /// a file in a folder the user picks, and show it in the Finder.
    ExportUi,
    /// Show the daemon's feedback file, where `leave-feedback` saves
    /// notes, in the Finder.
    ShowFeedbackFile,
    /// Open the feedback entry, as `leave-feedback` does, listing every
    /// recent note, the resolved ones with how they were resolved.
    ShowAllFeedback,
    /// Open the text size pop-up at the pen: the size a new text starts at.
    PickSize,
    /// Make a new text's size the next of the text sizes, or the one
    /// before, without a pop-up. At either end it stays.
    SizeUp,
    SizeDown,
    /// Select every item on the focused pane's page; in a text field or
    /// an agent UI's page, its own select all.
    SelectAll,
    /// Select the next item on the focused pane's page in reading order,
    /// or the one before, alone, and bring it into view.
    NextItem,
    PreviousItem,
    /// Move the selection a step that way, in one undo step.
    NudgeLeft,
    NudgeRight,
    NudgeUp,
    NudgeDown,
    /// Copy the selection beside itself, in one undo step; the copy is
    /// the selection.
    Duplicate,
    /// Find typed text on the focused pane's canvas: an entry asks for the
    /// words, and the pane turns to the first text that holds them, on its
    /// page or a page after it, centred.
    Find,
    /// Go to the next text that holds the last find's words, round to the
    /// first; with no find yet, as `find`.
    FindNext,
    /// Start a new text without a tap: under the text being typed, which
    /// ends; else at the pointer over the pane, or under the last text ended.
    NewText,
    /// Move the selection a point that way, in one undo step.
    NudgeLeftFine,
    NudgeRightFine,
    NudgeUpFine,
    NudgeDownFine,
}

/// How many commands there are.
const COUNT: usize = Command::ALL.len();

impl Command {
    /// Each command and its name in the config file, in declaration order.
    pub const ALL: [(Command, &str); 81] = [
        (Command::Quit, "quit"),
        (Command::ReloadConfig, "reload-config"),
        (Command::PickColour, "pick-colour"),
        (Command::PickShape, "pick-shape"),
        (Command::PickTool, "pick-tool"),
        (Command::PickWidth, "pick-width"),
        (Command::SplitRight, "split-right"),
        (Command::SplitDown, "split-down"),
        (Command::ClosePane, "close-pane"),
        (Command::FocusNext, "focus-next"),
        (Command::FocusLeft, "focus-left"),
        (Command::FocusRight, "focus-right"),
        (Command::FocusUp, "focus-up"),
        (Command::FocusDown, "focus-down"),
        (Command::GrowPane, "grow-pane"),
        (Command::ShrinkPane, "shrink-pane"),
        (Command::ToggleSidebar, "toggle-sidebar"),
        (Command::DismissNotice, "dismiss-notice"),
        (Command::PanLeft, "pan-left"),
        (Command::PanRight, "pan-right"),
        (Command::PanUp, "pan-up"),
        (Command::PanDown, "pan-down"),
        (Command::ZoomIn, "zoom-in"),
        (Command::ZoomOut, "zoom-out"),
        (Command::ZoomReset, "zoom-reset"),
        (Command::NextPage, "next-page"),
        (Command::DeleteSelection, "delete-selection"),
        (Command::ClearSelection, "clear-selection"),
        (Command::ToolPen, "tool-pen"),
        (Command::ToolEraser, "tool-eraser"),
        (Command::ToolLasso, "tool-lasso"),
        (Command::ToolText, "tool-text"),
        (Command::ToolBox, "tool-box"),
        (Command::ToolArrow, "tool-arrow"),
        (Command::ToolSelect, "tool-select"),
        (Command::WidthUp, "width-up"),
        (Command::WidthDown, "width-down"),
        (Command::Sling, "sling"),
        (Command::PreviousPage, "previous-page"),
        (Command::NewPage, "new-page"),
        (Command::RenamePage, "rename-page"),
        (Command::RemovePage, "remove-page"),
        (Command::Undo, "undo"),
        (Command::Redo, "redo"),
        (Command::Dictate, "dictate"),
        (Command::ShowKeys, "show-keys"),
        (Command::ShowCommands, "show-commands"),
        (Command::LeaveFeedback, "leave-feedback"),
        (Command::ToggleGrid, "toggle-grid"),
        (Command::SlingNow, "sling-now"),
        (Command::Save, "save"),
        (Command::ShowCanvasFile, "show-canvas-file"),
        (Command::OpenPdf, "open-pdf"),
        (Command::CropPdf, "crop-pdf"),
        (Command::OpenWorkspace, "open-workspace"),
        (Command::CloseWorkspace, "close-workspace"),
        (Command::ShowWorkspaceFolder, "show-workspace-folder"),
        (Command::StartAgent, "start-agent"),
        (Command::StopAgent, "stop-agent"),
        (Command::RestartAgent, "restart-agent"),
        (Command::ExportUi, "export-ui"),
        (Command::ShowFeedbackFile, "show-feedback-file"),
        (Command::ShowAllFeedback, "show-all-feedback"),
        (Command::PickSize, "pick-size"),
        (Command::SizeUp, "size-up"),
        (Command::SizeDown, "size-down"),
        (Command::SelectAll, "select-all"),
        (Command::NextItem, "next-item"),
        (Command::PreviousItem, "previous-item"),
        (Command::NudgeLeft, "nudge-left"),
        (Command::NudgeRight, "nudge-right"),
        (Command::NudgeUp, "nudge-up"),
        (Command::NudgeDown, "nudge-down"),
        (Command::Duplicate, "duplicate"),
        (Command::Find, "find"),
        (Command::FindNext, "find-next"),
        (Command::NewText, "new-text"),
        (Command::NudgeLeftFine, "nudge-left-fine"),
        (Command::NudgeRightFine, "nudge-right-fine"),
        (Command::NudgeUpFine, "nudge-up-fine"),
        (Command::NudgeDownFine, "nudge-down-fine"),
    ];

    /// The command called `name` in the config file.
    pub fn named(name: &str) -> Option<Command> {
        Self::ALL.iter().find(|(_, n)| *n == name).map(|&(c, _)| c)
    }
}

/// The modifier bits of a [`Chord`].
pub const SHIFT: u8 = 1;
pub const CTRL: u8 = 2;
pub const ALT: u8 = 4;
pub const CMD: u8 = 8;

/// A key with its modifiers: `"cmd+shift+k"` in the config file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Chord {
    mods: u8,
    key: char,
}

impl Chord {
    /// A key event: `mods` is a set of the bits above (others are dropped),
    /// and `key` is the character the key types with no modifier but shift.
    /// A letter matches in either case.
    pub const fn new(mods: u8, key: char) -> Chord {
        // Shift+tab comes as the back-tab character.
        let key = if key == '\u{19}' { '\t' } else { key.to_ascii_lowercase() };
        Chord { mods: mods & (SHIFT | CTRL | ALT | CMD), key }
    }

    /// The key, lowercase: `'k'` for `cmd+shift+k`.
    pub const fn key(self) -> char {
        self.key
    }

    /// Whether the key types text: neither command nor control is held.
    pub const fn types(self) -> bool {
        self.mods & (CTRL | CMD) == 0
    }
}

/// Which chord runs each command. A command may have none; a chord runs at
/// most one command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Keymap([Option<Chord>; COUNT]);

impl Default for Keymap {
    /// The few defaults; `config.example.toml` lists them. A command with
    /// none runs from the palette and the menu bar until the user binds
    /// one. The final keys wait for a canvas to use them on (SPEC O4).
    fn default() -> Self {
        let on = |mods, key| Some(Chord { mods, key });
        Keymap([
            on(CMD, 'q'),
            on(CMD, 'r'),
            on(CMD | SHIFT, 'c'),
            on(CMD | SHIFT, 's'),
            on(CMD | SHIFT, 't'),
            on(CMD | SHIFT, 'w'),
            on(CMD, 'd'),
            on(CMD | SHIFT, 'd'),
            on(CMD, 'w'),
            on(CTRL, '\t'),
            on(CMD | ALT, LEFT),
            on(CMD | ALT, RIGHT),
            on(CMD | ALT, UP),
            on(CMD | ALT, DOWN),
            on(ALT, '='),
            on(ALT, '-'),
            on(CMD, 'b'),
            on(CMD, '.'),
            on(ALT, LEFT),
            on(ALT, RIGHT),
            on(ALT, UP),
            on(ALT, DOWN),
            on(CMD, '='),
            on(CMD, '-'),
            on(CMD, '0'),
            on(CMD, ']'),
            on(0, '\u{7f}'),
            on(0, '\u{1b}'),
            on(CMD, '1'),
            on(CMD, '2'),
            on(CMD, '3'),
            on(CMD, '4'),
            on(CMD, '5'),
            on(CMD, '6'),
            on(CMD, '7'),
            on(ALT, ']'),
            on(ALT, '['),
            on(CMD, '\r'),
            on(CMD, '['),
            on(CMD, 'n'),
            on(CMD | SHIFT, 'r'),
            on(CMD | SHIFT, '\u{7f}'),
            on(CMD, 'z'),
            on(CMD | SHIFT, 'z'),
            on(CMD | SHIFT, '\r'),
            on(CMD, '/'),
            on(CMD, 'k'),
            on(CMD | SHIFT, 'f'),
            on(CMD, '\''),
            on(ALT, '\r'),
            on(CMD, 's'),
            on(CMD | ALT, 'r'),
            on(CMD, 'o'),
            on(CMD, '8'),
            on(CMD | SHIFT, 'o'),
            on(CMD | ALT, 'w'),
            None,
            on(CMD | ALT, 'a'),
            on(CMD | ALT, '.'),
            on(CMD | ALT | SHIFT, 'a'),
            None,
            None,
            None,
            on(CMD | SHIFT, 'a'),
            on(ALT | SHIFT, '}'),
            on(ALT | SHIFT, '{'),
            on(CMD, 'a'),
            on(0, '\t'),
            on(SHIFT, '\t'),
            on(0, LEFT),
            on(0, RIGHT),
            on(0, UP),
            on(0, DOWN),
            on(CMD, 'j'),
            on(CMD, 'f'),
            on(CMD, 'g'),
            on(CTRL, '\r'),
            on(SHIFT, LEFT),
            on(SHIFT, RIGHT),
            on(SHIFT, UP),
            on(SHIFT, DOWN),
        ])
    }
}

impl Keymap {
    /// The command `chord` runs, if any. A "+" pressed with shift runs what
    /// "=" is on when it has no key of its own: cmd++ is cmd+=, as a
    /// tablet's zoom wheel may send it.
    pub fn command(&self, chord: Chord) -> Option<Command> {
        let on = |chord| self.0.iter().position(|&c| c == Some(chord));
        let plus = chord.key == '+' && chord.mods & SHIFT != 0;
        let equals = Chord { mods: chord.mods & !SHIFT, key: '=' };
        let i = on(chord).or_else(|| plus.then(|| on(equals)).flatten())?;
        Some(Command::ALL[i].0)
    }

    /// The chord that runs `command`, if any: the live key a surface shows.
    pub fn chord(&self, command: Command) -> Option<Chord> {
        self.0[command as usize]
    }

    /// Puts `command` on `chord`, or on none. A command that was on `chord`
    /// loses it.
    pub fn bind(&mut self, command: Command, chord: Option<Chord>) {
        if chord.is_some() {
            for slot in &mut self.0 {
                if *slot == chord {
                    *slot = None;
                }
            }
        }
        self.0[command as usize] = chord;
    }
}

#[cfg(test)]
mod tests;
