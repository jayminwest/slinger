//! SPIKE (slinger-dd9b7b): the text being typed, shown as Nvim's cell grid
//! while an Nvim edits it ([`crate::nvim`]). Pure: `canvases` hands in each
//! grid Nvim flushed and the buffer's text as it changes; the scene shows
//! the grid in place of the text, its caret and its selection, and a press
//! inside is a cell's ([`Shapes::cell_at`]).

use std::borrow::Cow;
use std::sync::Arc;

use super::{EDITING, FRAME_ID, FRAME_LINE, FRAME_PAD, HINT_GAP, HINT_ID, Shapes, label};
use crate::config::Theme;
use crate::layout::{Point, Rect};
use crate::nvim::COLUMNS;
use crate::nvim::grid::Grid;
use crate::scene::canvas::{Item, Shape};
use crate::scene::editor::Editor;
use crate::scene::glyphs;

/// Under the grid, how the text ends.
const HINT: &str = "esc in normal mode, :q or a tap outside ends the text";

impl Shapes {
    /// An Nvim edits the text being typed from now on. A new text wraps in
    /// the grid's column, so it reads back as it was typed.
    pub fn vim_opened(&mut self) {
        self.vim = true;
        if let Some(editing) = self.editing.as_mut().filter(|e| e.item.is_none()) {
            #[expect(clippy::cast_precision_loss, reason = "80 columns")]
            let width = COLUMNS as f64 * editing.size * glyphs::CELL;
            editing.column = glyphs::column(width, editing.size);
        }
    }

    /// Whether an Nvim edits the text being typed.
    pub fn vimming(&self) -> bool {
        self.vim
    }

    /// The grid Nvim flushed last.
    pub fn set_grid(&mut self, grid: Arc<Grid>) {
        self.grid = Some(grid);
    }

    /// The buffer's text is `text` now: the text ends with it.
    pub fn set_text(&mut self, text: String) {
        if let Some(editing) = &mut self.editing {
            editing.editor = Editor::new(text);
        }
    }

    /// The characters of the text being typed, if any.
    pub fn typing_text(&self) -> Option<String> {
        self.editing.as_ref().map(|e| e.editor.text().to_string())
    }

    /// The box the grid fills, in canvas units, when one shows.
    #[expect(clippy::cast_precision_loss, reason = "a grid's cells are far below 2^52")]
    pub(super) fn grid_frame(&self) -> Option<Rect> {
        let (editing, grid) = self.editing.as_ref().zip(self.grid.as_ref())?;
        let (width, height) = (editing.size * glyphs::CELL, editing.size);
        Some(Rect {
            x: editing.at.x,
            y: editing.at.y,
            width: grid.cols as f64 * width,
            height: grid.rows as f64 * height,
        })
    }

    /// The grid's cell (row, column) at `at` on `page` of `canvas`, or the
    /// nearest when `at` is within `reach` of the grid; `None` outside it,
    /// and when no grid shows there.
    #[expect(clippy::cast_possible_truncation, reason = "clamped to the grid")]
    #[expect(clippy::cast_sign_loss, reason = "clamped to the grid")]
    pub fn cell_at(
        &self,
        (canvas, page): (&str, &str),
        at: Point,
        reach: f64,
    ) -> Option<(usize, usize)> {
        let editing = self.editing.as_ref().filter(|e| e.canvas == canvas && e.page == page)?;
        let (frame, grid) = self.grid_frame().zip(self.grid.as_ref())?;
        if !frame.outset(reach).contains(at) {
            return None;
        }
        let col = ((at.x - frame.x) / (editing.size * glyphs::CELL)).floor().max(0.0) as usize;
        let row = ((at.y - frame.y) / editing.size).floor().max(0.0) as usize;
        Some((row.min(grid.rows.saturating_sub(1)), col.min(grid.cols.saturating_sub(1))))
    }

    /// The grid in place of the text being typed, with the dashed frame
    /// and the hint, when a grid shows. False when none does.
    pub(super) fn cells(&self, items: &mut Vec<Cow<'_, Item>>, zoom: f64, theme: &Theme) -> bool {
        let Some(((editing, grid), frame)) =
            self.editing.as_ref().zip(self.grid.as_ref()).zip(self.grid_frame())
        else {
            return false;
        };
        let shape = Shape::Cells { frame, size: editing.size, grid: Arc::clone(grid) };
        items.push(Cow::Owned(Item { id: EDITING, colour: editing.colour, shape }));
        let around = frame.outset(FRAME_PAD / zoom);
        let shape =
            Shape::Panel { frame: around, fill: None, width: FRAME_LINE / zoom, dashed: true };
        items.push(Cow::Owned(Item { id: FRAME_ID, colour: theme.muted, shape }));
        let under = Point { x: around.x, y: around.y + around.height + HINT_GAP / zoom };
        items.push(Cow::Owned(label(HINT_ID, HINT.to_string(), theme.muted, under, zoom)));
        true
    }
}
