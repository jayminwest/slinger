//! The canvas text being typed (docs/design/pen-less.md, pick 2A): its
//! characters, a caret and a selection inside them, moved by the keys a Mac
//! text field moves them by. Pure: [`crate::scene::shapes`] hands in typed
//! and pasted text, the keys and presses inside the text, and draws the
//! caret bar and the selection's boxes this module gives, from
//! [`glyphs::Laid`], the rows every hit test and the core use.
//!
//! The caret and the anchor are byte offsets on grapheme cluster boundaries:
//! the caret moves, and delete deletes, by what reads as one character (an
//! emoji joined by ZWJ, a letter with its combining marks, a flag), not by
//! Unicode scalar. A word is a run of clusters that start with an
//! alphanumeric character or `_`.
//!
//! The keys ([`Editor::key`]) are fixed text keys, not commands: while a
//! text is typed they come before the keymap, so alt+arrows move by words
//! instead of panning, and cmd+A selects the text's characters.

use unicode_segmentation::{GraphemeCursor, UnicodeSegmentation};

use crate::commands::{
    ALT, CMD, CTRL, Chord, DELETE, DOWN, END, FORWARD_DELETE, HOME, LEFT, RIGHT, SHIFT, UP,
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
    /// What ctrl+K last took out, for ctrl+Y ([`emacs`]).
    killed: String,
}

/// How far a key moves or deletes: by a character, a word, to the row's
/// end (cmd), or to the line's end (ctrl+A, ctrl+E: a wrapped line's rows
/// are one line).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Unit {
    Char,
    Word,
    Row,
    Line,
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
        Editor { caret: text.len(), text, ..Editor::default() }
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

    /// Selects from `a` to `b`, the caret at `b`.
    fn select(&mut self, a: usize, b: usize) {
        self.place(a, false);
        self.place(b, true);
    }

    /// Selects the word at `index` (a double tap): the run of word
    /// clusters, or of other clusters on its line, it is in or beside; a
    /// word beside it wins over a space.
    fn select_word(&mut self, index: usize) {
        let class = |g: &str| {
            if g == "\n" {
                0
            } else if word(g) {
                1
            } else {
                2
            }
        };
        let ahead = self.text[index..].graphemes(true).next();
        let behind = self.text[..index].graphemes(true).next_back();
        let of = match (behind, ahead) {
            (Some(b), a) if word(b) && !a.is_some_and(word) => Some(class(b)),
            (_, Some(a)) if a != "\n" => Some(class(a)),
            (b, _) => b.filter(|&b| b != "\n").map(class),
        };
        // An empty line has no word: the caret only goes there.
        let Some(of) = of else { return self.place(index, false) };
        let back = self.text[..index].grapheme_indices(true).rev();
        let start = back.take_while(|&(_, g)| class(g) == of).last().map_or(index, |(i, _)| i);
        let rest = &self.text[index..];
        let end = rest.grapheme_indices(true).find(|&(_, g)| class(g) != of);
        self.select(start, index + end.map_or(rest.len(), |(i, _)| i));
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
                '\u{7f}' | '\u{8}' => self.delete(before(&self.text, self.caret)),
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
    /// delete; cmd+A selects all; ctrl and a letter is a Mac's emacs key
    /// ([`emacs`]). Returns false for any other key, which goes on to the
    /// keymap.
    pub fn key(&mut self, chord: Chord, size: f64, column: f64) -> bool {
        let extend = chord.mods() & SHIFT != 0;
        let unit = match chord.mods() & !SHIFT {
            0 => Unit::Char,
            ALT => Unit::Word,
            CMD => Unit::Row,
            CTRL => return self.emacs(chord.key(), extend, size, column),
            _ => return false,
        };
        self.apply(chord.key(), unit, extend, size, column)
    }

    /// Does what `key` with `unit` held does ([`Editor::act`]); false when
    /// the key is not the text's.
    fn apply(&mut self, key: char, unit: Unit, extend: bool, size: f64, column: f64) -> bool {
        let Some(act) = self.act(key, unit, extend, size, column) else { return false };
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
        match unit {
            Unit::Char => before(&self.text, self.caret),
            Unit::Word => {
                let clusters = self.text[..self.caret].grapheme_indices(true).rev();
                let mut clusters = clusters.skip_while(|&(_, g)| !word(g));
                let mut start = clusters.next().map_or(0, |(i, _)| i);
                for (i, _) in clusters.take_while(|&(_, g)| word(g)) {
                    start = i;
                }
                start
            }
            Unit::Row => {
                let row = laid.rows()[laid.row_of(self.caret)].start;
                // At a row's start, back over the line end before it.
                if row == self.caret { before(&self.text, self.caret) } else { row }
            }
            Unit::Line => line_start(&self.text, self.caret),
        }
    }

    /// Where the caret goes forward to by `unit`.
    fn forward(&self, unit: Unit, laid: &Laid<'_>) -> usize {
        match unit {
            Unit::Char => after(&self.text, self.caret),
            Unit::Word => {
                let rest = &self.text[self.caret..];
                let mut clusters = rest.grapheme_indices(true).skip_while(|&(_, g)| !word(g));
                let end = clusters.find(|&(_, g)| !word(g)).map_or(rest.len(), |(i, _)| i);
                self.caret + end
            }
            Unit::Row => laid.rows()[laid.row_of(self.caret)].end,
            Unit::Line => line_end(&self.text, self.caret),
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

/// A cluster of a word: one that starts with an alphanumeric character or `_`.
fn word(cluster: &str) -> bool {
    cluster.chars().next().is_some_and(|c| c.is_alphanumeric() || c == '_')
}

/// The start of the line `index` is in: after the line end before it.
fn line_start(text: &str, index: usize) -> usize {
    text[..index].rfind('\n').map_or(0, |i| i + 1)
}

/// The end of the line `index` is in: its line end, or the text's end.
fn line_end(text: &str, index: usize) -> usize {
    text[index..].find('\n').map_or(text.len(), |i| index + i)
}

/// The grapheme cluster boundary before `index` in `text`, or its start.
fn before(text: &str, index: usize) -> usize {
    let mut cursor = GraphemeCursor::new(index, text.len(), true);
    cursor.prev_boundary(text, 0).ok().flatten().unwrap_or(0)
}

/// The grapheme cluster boundary after `index` in `text`, or its end.
fn after(text: &str, index: usize) -> usize {
    let mut cursor = GraphemeCursor::new(index, text.len(), true);
    cursor.next_boundary(text, 0).ok().flatten().unwrap_or(text.len())
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
    /// for new texts (`crate::scene::shapes::Shapes::set_size`), until a
    /// size command resizes it (`crate::scene::shapes::Shapes::resize`).
    pub size: f64,
    /// The column it wraps in: an edited text's own, so an agent's
    /// paragraph keeps its rows ([`glyphs::UNWRAPPED`] for one typed
    /// before texts wrapped); a new one's holds [`glyphs::NEW_COLUMN`]
    /// characters. A drag of its frame's edge sets it ([`Editing::widen`]).
    pub column: f64,
    /// The text item edited, and its text, size and column before; `None`
    /// for a new text.
    pub item: Option<(ItemId, String, f64, f64)>,
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
    /// text, its size or its column.
    pub fn changed(&self) -> bool {
        self.item.as_ref().is_none_or(|(_, text, size, column)| {
            text != self.editor.text()
                || size.total_cmp(&self.size).is_ne()
                || column.total_cmp(&self.column).is_ne()
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

    /// The `taps`th quick press in a row inside the text, at the canvas
    /// point `at`: the first places the caret ([`Editing::place`], `shift`
    /// extending), the second selects the word there, a third or more the
    /// row.
    pub fn tap(&mut self, at: Point, shift: bool, taps: u8) {
        let laid = self.laid();
        let index = laid.index_at(at);
        let row = laid.rows()[laid.row_of(index)];
        match taps {
            0 | 1 => self.editor.place(index, shift),
            2 => self.editor.select_word(index),
            _ => self.editor.select(row.start, row.end),
        }
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

mod emacs;
#[cfg(test)]
mod tests;
