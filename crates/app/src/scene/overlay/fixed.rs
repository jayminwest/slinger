//! The keys and gestures the key overlay lists that are not commands: the
//! caret keys of a canvas text being typed ([`crate::scene::editor`],
//! docs/design/pen-less.md pick 2A) and the pans held rather than pressed
//! (pick 1). They are fixed, so no config moves them and the registry does
//! not list them; the overlay shows them after the command groups, as rows
//! with no target.

use crate::commands::Chord;

/// A heading and its rows of fixed keys.
#[derive(Debug, PartialEq, Eq)]
pub struct Fixed {
    pub title: &'static str,
    pub rows: &'static [Row],
}

/// One fixed row: what it does, and its keys.
#[derive(Debug, PartialEq, Eq)]
pub struct Row {
    pub title: &'static str,
    pub keys: Keys,
}

/// A row's keys: chords as config writes them, split by spaces, that a
/// typed text's editor takes; or words shown as they are.
#[derive(Debug, PartialEq, Eq)]
pub enum Keys {
    Text(&'static str),
    Words(&'static str),
}

impl Keys {
    /// What the row's chip says: each chord in macOS glyphs ("⌥← ⌥→").
    pub fn shown(&self) -> String {
        match self {
            Keys::Text(chords) => {
                let glyphs = chords.split(' ').filter_map(Chord::parse);
                glyphs.map(Chord::glyphs).collect::<Vec<_>>().join(" ")
            }
            Keys::Words(words) => (*words).into(),
        }
    }
}

const fn row(title: &'static str, keys: Keys) -> Row {
    Row { title, keys }
}

/// Every fixed section, in the order the overlay shows them.
pub const ALL: [&Fixed; 2] = [&TYPING, &POINTER];

/// The keys of a canvas text being typed: they come before the keymap.
pub const TYPING: Fixed = Fixed {
    title: "Typing a text",
    rows: &[
        row("Caret a character", Keys::Text("left right up down")),
        row("Caret a word", Keys::Text("alt+left alt+right")),
        row("Caret to row's ends", Keys::Text("cmd+left cmd+right")),
        row("Caret to text's ends", Keys::Text("cmd+up cmd+down")),
        row("Select as it moves", Keys::Text("shift+left shift+right")),
        row("Delete a word, a row", Keys::Text("alt+delete cmd+delete")),
        row("Delete forward", Keys::Text("forward-delete alt+forward-delete")),
        row("Select the whole text", Keys::Text("cmd+a")),
        row("Line start, line end", Keys::Text("ctrl+a ctrl+e")),
        row("Cut line end, put back", Keys::Text("ctrl+k ctrl+y")),
        row("Select a word, a row", Keys::Words("2 or 3 taps")),
        row("Copy, cut selected text", Keys::Words("⌘C ⌘X")),
        row("Undo, redo its edits", Keys::Words("⌘Z ⇧⌘Z")),
        row("End the text", Keys::Words("⎋ or tap out")),
    ],
};

/// The pans and zoom a mouse or trackpad makes without a chord.
pub const POINTER: Fixed = Fixed {
    title: "Pointer",
    rows: &[
        row("Pan, space held", Keys::Words("Space drag")),
        row("Pan, middle or right", Keys::Words("drag")),
        row("Zoom at the pointer", Keys::Words("⌘ scroll")),
    ],
};
