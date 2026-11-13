//! Reader mode on a pane (docs/design/pdf.md): a pane on a PDF's canvas
//! ([`pdf::reader`]) fits the page's column of `pdf` items ([`pdf::column`])
//! to its width each time it draws, so a resize refits it, and its scroll
//! moves the pages up and down only, no further than the column's ends
//! ([`Viewport::reading`]). Its zoom does not change. A page with no `pdf`
//! item has no column, and pans and zooms as any canvas.

use super::View;
use crate::layout::Size;
use crate::scene::canvas::Item;
use crate::scene::pdf;

impl View {
    /// Fits this view, on `canvas` showing `items` (`None` while its page
    /// is on its way) on a screen of `size`, to its column when it is in
    /// reader mode, and remembers whether it is. A page on its way keeps
    /// the column it had.
    pub(super) fn read(&mut self, canvas: &str, items: Option<&[Item]>, size: Size) {
        let column = match items {
            _ if !pdf::reader(canvas) => None,
            Some(items) => pdf::column(items),
            None => self.reader.map(|(column, _)| column),
        };
        self.reader = column.map(|column| (column, size));
        if let Some(column) = column {
            self.viewport = self.viewport.reading(column, size);
        }
    }

    /// Drags the canvas by `dx`, `dy` screen points: up and down only, and
    /// no further than the column's ends, in reader mode.
    pub(super) fn pan(&mut self, dx: f64, dy: f64) {
        self.viewport = match self.reader {
            Some((column, size)) => self.viewport.pan(0.0, dy).reading(column, size),
            None => self.viewport.pan(dx, dy),
        };
    }
}

#[cfg(test)]
mod tests;
