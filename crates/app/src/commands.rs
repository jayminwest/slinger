//! Key commands (SPEC P4): every action the app takes from the keyboard is a
//! named [`Command`], bound to a [`Chord`] in the config file. A tablet button
//! is a key, set in the tablet's own software. Pure: `platform` turns a key
//! event into a chord, and a [`Keymap`] turns the chord into a command.
//! `table` gives each command its name, title, group and default chord in
//! one row; `registry` the groups and a chord's glyphs for the surfaces that
//! list them; `menu` lays them out as the menu bar; `able` says whether a
//! command can act now (the bar and the context menus grey by it);
//! `context` builds the context menus.

pub mod able;
mod chord;
pub mod context;
pub mod menu;
mod registry;
mod table;

use chord::F1;
pub use chord::{DELETE, DOWN, END, FORWARD_DELETE, HOME, LEFT, RIGHT, UP};
pub use registry::Group;
use table::TABLE;

/// Every action a key can run. Each has its row in the command table
/// (`table.rs`): its name, title, group and default chord.
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
    /// Split the focused pane; the new pane, on the right, shows the same,
    /// at the same place (page, pan, zoom, box).
    SplitRight,
    /// Split the focused pane; the new pane, below, shows the same place.
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
    /// Hide the notice that an agent is blocked or done, and the offer to
    /// restart a slingerd of another build.
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
    /// Undo the user's last step on the focused pane's canvas; an agent's steps are its own.
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
    /// Show the daemon's feedback file, where `leave-feedback` saves notes, in the Finder.
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
    /// Select every item on the focused pane's page; in a text field or an agent UI's page,
    /// its own select all.
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
    /// Copy the selection beside itself, in one undo step; the copy is the selection.
    Duplicate,
    /// Find typed text on the focused pane's canvas: an entry asks for the words, and the
    /// pane turns to the first text that holds them, on its page or a page after it, centred.
    Find,
    /// Go to the next text that holds the last find's words, round to the
    /// first; with no find yet, as `find`.
    FindNext,
    /// Start a new text without a tap: under the text being typed, which
    /// ends; else at the pointer over the pane, or under the last text ended.
    NewText,
    /// Fold each selected box to a miniature, or unfold a folded one.
    Fold,
    /// Move the selection a point that way, in one undo step.
    NudgeLeftFine,
    NudgeRightFine,
    NudgeUpFine,
    NudgeDownFine,
    /// Step the focused pane into the selected box: it shows only what the
    /// box holds, until a step out. Per pane, like its page.
    StepIn,
    /// Split the focused pane and step the new pane into the selected box.
    StepInBeside,
    /// Step the focused pane out of the box it is in, one level.
    StepOut,
    /// Start the bundled slingerd that was stopped on purpose (a bundled
    /// app only: `crate::launch`).
    StartSlingerd,
    /// Stop slingerd and start the bundled one: what the offer for a
    /// slingerd of another build runs (a bundled app only: `crate::launch`).
    RestartDaemon,
    /// Zoom and pan the focused pane's canvas so all its page shows (the
    /// room, inside a box), at 100% or less.
    ZoomToFit,
    /// Load the agent UI the focused pane shows again, from a fresh fetch
    /// of its showing: its page starts over, as a new one does.
    ReloadUi,
    /// Grab the figure under the pointer, an agent's locked drawing, as one
    /// selection: move, duplicate or delete it whole. It locks again when
    /// the selection ends (docs/design/figures.md, "The unlock").
    UnlockFigure,
}

/// How many commands there are.
const COUNT: usize = TABLE.len();

impl Command {
    /// Each command and its name in the config file, in declaration order.
    pub const ALL: [(Command, &str); COUNT] = {
        let mut all = [(Command::Quit, ""); COUNT];
        let mut i = 0;
        while i < COUNT {
            all[i] = (TABLE[i].command, TABLE[i].name);
            i += 1;
        }
        all
    };

    /// The command called `name` in the config file.
    pub fn named(name: &str) -> Option<Command> {
        TABLE.iter().find(|row| row.name == name).map(|row| row.command)
    }

    /// The command's name for people: "Split right".
    pub const fn title(self) -> &'static str {
        TABLE[self as usize].title
    }

    /// The group the command shows under.
    pub const fn group(self) -> Group {
        TABLE[self as usize].group
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
    /// The few defaults, from the command table; `config.example.toml`
    /// lists them. The final keys wait for a canvas to use them on (SPEC O4).
    fn default() -> Self {
        Keymap(std::array::from_fn(|i| TABLE[i].key))
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
