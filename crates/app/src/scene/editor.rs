//! The canvas text being typed (docs/design/pen-less.md, pick 2A): its
//! characters, a caret and a selection inside them, moved by the keys a Mac
//! text field moves them by. Pure: [`crate::scene::shapes`] hands in typed
//! and pasted text, the keys and presses inside the text, and draws the
//! caret bar and the selection's boxes this module gives, from
//! [`glyphs::Laid`], the rows every hit test and the core use.
//!
//! The caret and the anchor are byte offsets on `char` boundaries: the caret
//! moves by Unicode scalar, not by grapheme cluster (slinger-459ce9). A
//! word is a run of alphanumeric characters or `_`.
//!
//! The keys ([`Editor::key`]) are fixed text keys, not commands: while a
//! text is typed they come before the keymap, so alt+arrows move by words
//! instead of panning, and cmd+A selects the text's characters.

use crate::commands::{
    ALT, CMD, Chord, DELETE, DOWN, END, FORWARD_DELETE, HOME, LEFT, RIGHT, SHIFT, UP,
};
use crate::config::Rgb;
use crate::layout::{Point, Rect};
use crate::scene::canvas::ItemId;
use crate::scene::glyphs::{self, Laid};

/// A text's characters with a caret and a selection in them.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Editor {
    text: String,
    /// Where typing goes in, and the moving end of the selection.
    caret: usize,
    /// The selection's other end, when one is being made or is made.
    anchor: Option<usize>,
    /// The x, from the text's left edge, up and down keep to.
    goal: Option<f64>,
}

/// How far a key moves or deletes: by a character, a word, or to the row's
/// end (cmd).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Unit {
    Char,
    Word,
    Row,
}

/// What a key does: moves the caret (keeping an x for up and down), deletes
/// to an index, or selects all.
enum Act {
    Move(usize, Option<f64>),
    Delete(usize),
    All,
}

impl Editor {
    /// `text`, with the caret after its last character.
    pub fn new(text: String) -> Self {
        Editor { caret: text.len(), text, anchor: None, goal: None }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn caret(&self) -> usize {
        self.caret
    }

    /// The selection's start and end, when it holds a character.
    pub fn selection(&self) -> Option<(usize, usize)> {
        let anchor = self.anchor.filter(|&a| a != self.caret)?;
        Some((anchor.min(self.caret), anchor.max(self.caret)))
    }

    /// The selected characters, if any.
    pub fn selected(&self) -> Option<&str> {
        self.selection().map(|(a, b)| &self.text[a..b])
    }

    /// Takes the selected characters out and gives them back; `None`, and
    /// nothing changes, when none are.
    pub fn cut(&mut self) -> Option<String> {
        let (a, b) = self.selection()?;
        let cut = self.text[a..b].to_string();
        self.replace(a, b, "");
        Some(cut)
    }

    /// `text` goes in at the caret, in place of the selection if any.
    pub fn insert(&mut self, text: &str) {
        let (a, b) = self.selection().unwrap_or((self.caret, self.caret));
        self.replace(a, b, text);
    }

    /// Selects every character.
    pub fn select_all(&mut self) {
        self.anchor = Some(0);
        self.caret = self.text.len();
        self.goal = None;
    }

    /// Puts the caret at `index` (a press inside the text); with `extend`
    /// (shift, or a drag) the selection runs to it from its anchor, or
    /// from where the caret was.
    pub fn place(&mut self, index: usize, extend: bool) {
        self.anchor = if extend { Some(self.anchor.unwrap_or(self.caret)) } else { None };
        self.caret = index.min(self.text.len());
        self.goal = None;
    }

    /// Deletes the selection, else from the caret to `to`.
    fn delete(&mut self, to: usize) {
        let (a, b) = self.selection().unwrap_or((to.min(self.caret), to.max(self.caret)));
        self.replace(a, b, "");
    }

    /// Typed `text` (the characters a key typed) does what a text field does:
    /// delete deletes back a character, return starts a line; other control
    /// and function keys do nothing.
    pub fn typed(&mut self, text: &str) {
        for c in text.chars() {
            match c {
                '\u{7f}' | '\u{8}' => {
                    let to =
                        self.text[..self.caret].char_indices().next_back().map_or(0, |(i, _)| i);
                    self.delete(to);
                }
                '\r' | '\n' => self.insert("\n"),
                // The arrow and function keys are in this private range.
                c if c.is_control() || ('\u{f700}'..='\u{f8ff}').contains(&c) => {}
                c => self.insert(c.encode_utf8(&mut [0; 4])),
            }
        }
    }

    /// A key of the text's (docs/design/pen-less.md's table), for the text
    /// laid `size` high in `column`: the arrows, home and end move the
    /// caret (with shift, the selection's end); delete and forward delete
    /// delete; cmd+A selects all. Returns false for any other key, which
    /// goes on to the keymap.
    pub fn key(&mut self, chord: Chord, size: f64, column: f64) -> bool {
        let extend = chord.mods() & SHIFT != 0;
        let unit = match chord.mods() & !SHIFT {
            0 => Unit::Char,
            ALT => Unit::Word,
            CMD => Unit::Row,
            _ => return false,
        };
        let Some(act) = self.act(chord.key(), unit, extend, size, column) else { return false };
        match act {
            Act::Move(to, goal) => {
                self.place(to, extend);
                self.goal = goal;
            }
            Act::Delete(to) => self.delete(to),
            Act::All => self.select_all(),
        }
        true
    }

    /// What `key` with `unit` held does, read off the text laid `size` high
    /// in `column`; `None` when the key is not the text's.
    fn act(&self, key: char, unit: Unit, extend: bool, size: f64, column: f64) -> Option<Act> {
        let laid = Laid::new(&self.text, Point { x: 0.0, y: 0.0 }, size, column);
        let collapsed = self.selection().filter(|_| !extend);
        Some(match (key, unit) {
            (LEFT, _) => Act::Move(
                match collapsed.filter(|_| unit == Unit::Char) {
                    Some((a, _)) => a,
                    None => self.back(unit, &laid),
                },
                None,
            ),
            (RIGHT, _) => Act::Move(
                match collapsed.filter(|_| unit == Unit::Char) {
                    Some((_, b)) => b,
                    None => self.forward(unit, &laid),
                },
                None,
            ),
            (UP | DOWN, Unit::Char) => self.vertical(key == UP, collapsed, &laid),
            (UP | HOME, Unit::Row | Unit::Char) => Act::Move(0, None),
            (DOWN | END, Unit::Row | Unit::Char) => Act::Move(self.text.len(), None),
            (DELETE, _) => Act::Delete(self.back(unit, &laid)),
            (FORWARD_DELETE, Unit::Char | Unit::Word) => Act::Delete(self.forward(unit, &laid)),
            ('a', Unit::Row) => Act::All,
            _ => return None,
        })
    }

    /// Where the caret goes back to by `unit`.
    fn back(&self, unit: Unit, laid: &Laid<'_>) -> usize {
        let before = &self.text[..self.caret];
        let char_back = || before.char_indices().next_back().map_or(0, |(i, _)| i);
        match unit {
            Unit::Char => char_back(),
            Unit::Word => {
                let mut chars = before.char_indices().rev().skip_while(|&(_, c)| !word(c));
                let mut start = chars.next().map_or(0, |(i, _)| i);
                for (i, _) in chars.take_while(|&(_, c)| word(c)) {
                    start = i;
                }
                start
            }
            Unit::Row => {
                let row = laid.rows()[laid.row_of(self.caret)].start;
                // At a row's start, back over the line end before it.
                if row == self.caret { char_back() } else { row }
            }
        }
    }

    /// Where the caret goes forward to by `unit`.
    fn forward(&self, unit: Unit, laid: &Laid<'_>) -> usize {
        let after = &self.text[self.caret..];
        match unit {
            Unit::Char => self.caret + after.chars().next().map_or(0, char::len_utf8),
            Unit::Word => {
                let mut chars = after.char_indices().skip_while(|&(_, c)| !word(c));
                let end = chars.find(|&(_, c)| !word(c)).map_or(after.len(), |(i, _)| i);
                self.caret + end
            }
            Unit::Row => laid.rows()[laid.row_of(self.caret)].end,
        }
    }

    /// Up or down a row from the caret, or from the selection's start (up)
    /// or end (down) as it collapses, at the x the last up or down kept;
    /// from the first row up goes to the start, from the last down to the end.
    fn vertical(&self, up: bool, collapsed: Option<(usize, usize)>, laid: &Laid<'_>) -> Act {
        let from = collapsed.map_or(self.caret, |(a, b)| if up { a } else { b });
        let goal = self.goal.unwrap_or_else(|| laid.x(from));
        let row = laid.row_of(from);
        let to = match (up, row) {
            (true, 0) => 0,
            (true, row) => laid.index_in(row - 1, goal),
            (false, row) if row + 1 >= laid.rows().len() => self.text.len(),
            (false, row) => laid.index_in(row + 1, goal),
        };
        Act::Move(to, Some(goal))
    }

    /// `text[a..b]` becomes `with`, and the caret stands after it.
    fn replace(&mut self, a: usize, b: usize, with: &str) {
        self.text.replace_range(a..b, with);
        self.caret = a + with.len();
        self.anchor = None;
        self.goal = None;
    }
}

/// A character of a word: alphanumeric or `_`.
fn word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// A canvas text being typed: where it is, how it is set, the item it
/// edits, and its [`Editor`].
#[derive(Debug, Clone, PartialEq)]
pub struct Editing {
    pub canvas: String,
    pub page: String,
    /// The text's top left, in canvas units.
    pub at: Point,
    pub colour: Rgb,
    /// The text's size: an edited text's own, a new one's the size set
    /// for new texts (`crate::scene::shapes::Shapes::set_size`).
    pub size: f64,
    /// The column it wraps in: an edited text's own, so an agent's
    /// paragraph keeps its rows ([`glyphs::UNWRAPPED`] for one typed
    /// before texts wrapped); a new one's holds [`glyphs::NEW_COLUMN`]
    /// characters. A drag of its frame's edge sets it ([`Editing::widen`]).
    pub column: f64,
    /// The text item edited, and its text and column before; `None` for a
    /// new text.
    pub item: Option<(ItemId, String, f64)>,
    pub editor: Editor,
}

impl Editing {
    /// A new, empty text on `page` of `canvas` from `at`, its top left, in
    /// `colour` at `size`, in a column of [`glyphs::NEW_COLUMN`] characters.
    pub fn new(canvas: &str, page: &str, at: Point, colour: Rgb, size: f64) -> Self {
        let column = glyphs::column(glyphs::NEW_COLUMN * size * glyphs::CELL, size);
        let (canvas, page) = (canvas.to_string(), page.to_string());
        Editing { canvas, page, at, colour, size, column, item: None, editor: Editor::default() }
    }

    /// The frame it is shown in while typed: its glyphs' ([`Editing::frame`]),
    /// out to its column's right edge when it wraps, so the frame shows
    /// where its rows break.
    pub fn bounds(&self) -> Rect {
        let frame = self.frame();
        let column = if self.column < glyphs::UNWRAPPED { self.column } else { 0.0 };
        Rect { width: frame.width.max(column), ..frame }
    }

    /// The right edge of its shown frame ([`Editing::bounds`]) moves to
    /// about `right`, a canvas x: the column it wraps in becomes that wide
    /// ([`glyphs::column`]), at least a character.
    pub fn widen(&mut self, right: f64) {
        self.column = glyphs::column(right - self.at.x, self.size);
    }

    /// Whether ending it changes its item: a new text, or an edit of its
    /// text or its column.
    pub fn changed(&self) -> bool {
        self.item.as_ref().is_none_or(|(_, text, column)| {
            text != self.editor.text() || column.to_bits() != self.column.to_bits()
        })
    }

    /// The box its glyphs fill in its column, in the core's rows: the caret,
    /// the dashed frame and a press inside follow the glyphs. The file
    /// stores its own frame.
    pub fn frame(&self) -> Rect {
        glyphs::rows_frame(self.at, self.editor.text(), self.size, self.column)
    }

    /// Its text laid out where it is.
    fn laid(&self) -> Laid<'_> {
        Laid::new(self.editor.text(), self.at, self.size, self.column)
    }

    /// A key of the text's ([`Editor::key`]).
    pub fn key(&mut self, chord: Chord) -> bool {
        self.editor.key(chord, self.size, self.column)
    }

    /// A press (or, with `extend`, a shift-press or a drag) at the canvas
    /// point `at` puts the caret at the character boundary nearest it.
    pub fn place(&mut self, at: Point, extend: bool) {
        let index = self.laid().index_at(at);
        self.editor.place(index, extend);
    }

    /// The caret, `width` canvas units wide, and the boxes the selection
    /// fills, in canvas units.
    pub fn marks(&self, width: f64) -> (Rect, Vec<Rect>) {
        let laid = self.laid();
        let caret = laid.caret(self.editor.caret(), width);
        let boxes = self.editor.selection().map(|(a, b)| laid.selection(a, b, self.size / 4.0));
        (caret, boxes.unwrap_or_default())
    }
}

#[cfg(test)]
mod tests;
