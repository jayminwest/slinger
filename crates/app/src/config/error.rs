//! Why a config line is rejected, and what to do about it. Pure.

/// Why a line is rejected. The line itself names the table, key or value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    /// Neither a comment, a `[table]` nor `key = "value"`.
    Syntax,
    UnknownTable,
    UnknownKey,
    DuplicateKey,
    /// The value is not one double-quoted string.
    NotAString,
    BadColour,
    /// Not one to nine colours.
    BadPalette,
    /// Not one to nine widths, each above 0 and at most 50.
    BadWidths,
    /// `[window] sidebar` is neither `open` nor `closed`.
    BadSidebar,
    /// `[canvas] background` is neither `plain` nor `dots`.
    BadBackground,
    /// `[canvas] grid` is not a distance from 1 to 500 points.
    BadGrid,
    /// `[sling] prompt` is blank.
    BadPrompt,
    /// `[pointer] press` is neither `select` nor `tool`.
    BadPress,
    /// `[editor] nvim` is neither `auto`, `off` nor a path.
    BadNvim,
    /// A key in `[keys]` that names no command.
    UnknownCommand,
    BadChord,
    /// Two commands on one chord in one file.
    ChordTaken,
}

impl ErrorKind {
    /// What is wrong, and what to do about it.
    pub(super) fn text(self) -> (&'static str, &'static str) {
        let every = "config.example.toml lists every table and key";
        match self {
            Self::Syntax => (
                "not a table, a setting or a comment",
                "write a [table] or key = \"value\"; start a comment with #",
            ),
            Self::UnknownTable => ("unknown table", every),
            Self::UnknownKey => ("unknown key", every),
            Self::DuplicateKey => {
                ("the key is set twice", "set each key once; put machine values in the overlay")
            }
            Self::NotAString => {
                ("the value is not a quoted string", "quote the value: key = \"value\"")
            }
            Self::BadColour => ("the value is not a colour", "write a colour as \"#rrggbb\""),
            Self::BadPalette => (
                "the value is not one to nine colours",
                "write the colours as \"#rrggbb #rrggbb\", split by spaces or commas",
            ),
            Self::BadWidths => (
                "the value is not one to nine widths from 0.1 to 50",
                "write the widths in points as \"2 4 8 16\", split by spaces or commas",
            ),
            Self::BadSidebar => ("the value is not open or closed", "write \"open\" or \"closed\""),
            Self::BadBackground => {
                ("the value is not plain or dots", "write \"plain\" or \"dots\"")
            }
            Self::BadGrid => (
                "the value is not a distance from 1 to 500 points",
                "write the canvas points between dots, with at most two decimal places: \"28.35\"",
            ),
            Self::BadPrompt => (
                "the value is blank",
                "write the words a sling with no prompt sends: \"Read what I wrote and do what it asks.\"",
            ),
            Self::BadPress => ("the value is not select or tool", "write \"select\" or \"tool\""),
            Self::BadNvim => (
                "the value is not auto, off or a path",
                "write \"auto\", \"off\" or the path to nvim: \"/opt/homebrew/bin/nvim\"",
            ),
            Self::UnknownCommand => ("unknown command", every),
            Self::BadChord => (
                "the value is not a key chord",
                "write a chord as \"cmd+shift+k\", or \"\" for none",
            ),
            Self::ChordTaken => {
                ("another command is on this chord", "bind each chord to one command")
            }
        }
    }
}
