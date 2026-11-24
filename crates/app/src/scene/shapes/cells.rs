//! The text being typed, shown as Nvim's cell grid while an Nvim edits it
//! (docs/design/writing-column.md, "The grid"; `canvases::editing::vim`).
//! Pure: each grid Nvim flushed comes in ([`Shapes::set_grid`]); the scene
//! shows it in place of the text, its caret and its selection, from the
//! text's top left, a cell [`glyphs::CELL`] of the size wide and a size
//! tall, so the text keeps its place and its rows.
//!
//! The grid is opaque: it paints Nvim's own background over its whole box,
//! as the editor it is, so nothing under the text being typed shows through
//! it (the owner's colorscheme decides how it looks). What lies under it
//! shows again once the text ends.
//!
//! A press inside the grid is a cell's ([`Shapes::cell_at`]): Nvim's
//! mouse takes it (slinger-b8e3d3).

use std::borrow::Cow;
use std::sync::Arc;

use super::{EDITING, FRAME_ID, FRAME_LINE, FRAME_PAD, HINT_GAP, HINT_ID, Shapes, label};
use crate::config::Theme;
use crate::layout::{Point, Rect};
use crate::nvim::grid::Grid;
use crate::scene::canvas::{Item, Shape};
use crate::scene::glyphs;

/// Under the grid, how the text ends.
const HINT: &str = "esc in normal mode, :q or a tap outside ends the text";

impl Shapes {
    /// The grid Nvim flushed last, or none: the app's own editor shows the text.
    pub fn set_grid(&mut self, grid: Option<Arc<Grid>>) {
        self.grid = grid.filter(|_| self.editing.is_some());
    }

    /// The box the grid fills, in canvas units, when one shows.
    #[expect(clippy::cast_precision_loss, reason = "a grid's cells are far below 2^52")]
    pub(super) fn grid_frame(&self) -> Option<Rect> {
        let (editing, grid) = self.editing.as_ref().zip(self.grid.as_ref())?;
        let (width, height) = (editing.size * glyphs::CELL, editing.size);
        let (cols, rows) = (grid.cols as f64, grid.rows as f64);
        Some(Rect { x: editing.at.x, y: editing.at.y, width: cols * width, height: rows * height })
    }

    /// The grid's cell (row, column) at `at` on `page` of `canvas`, or the
    /// nearest one when `at` is within `reach` of the grid; `None` outside
    /// it, and when no grid shows there.
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
    /// and the hint, sized for `zoom` in `theme`'s colours. False when no
    /// grid shows.
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
        let hint = match self.next {
            Some(key) => format!("{HINT} · {} new text", key.glyphs()),
            None => HINT.to_string(),
        };
        items.push(Cow::Owned(label(HINT_ID, hint, theme.muted, under, zoom)));
        true
    }
}
