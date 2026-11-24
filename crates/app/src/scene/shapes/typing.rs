//! The text being typed as another editor sees it: an Nvim edits it
//! (`canvases::editing::vim`) and hands back its buffer. Pure.

use super::Shapes;
use crate::layout::Point;
use crate::scene::editor::Editor;

impl Shapes {
    /// The characters of the text being typed, if any.
    pub fn typing_text(&self) -> Option<String> {
        self.editing.as_ref().map(|e| e.editor.text().to_string())
    }

    /// How many texts have ended: the text being typed is another one
    /// when this changed.
    pub fn ends(&self) -> u64 {
        self.ends
    }

    /// Where the caret of the text being typed is, as a byte index: Nvim
    /// starts with its cursor there.
    pub fn typing_caret(&self) -> Option<usize> {
        self.editing.as_ref().map(|e| e.editor.caret())
    }

    /// Where the text being typed is: its canvas, page, top left (in
    /// canvas units) and size.
    pub fn typing_place(&self) -> Option<(&str, &str, Point, f64)> {
        let e = self.editing.as_ref()?;
        Some((e.canvas.as_str(), e.page.as_str(), e.at, e.size))
    }

    /// Whether the text being typed is a new one, not an item opened.
    pub fn typing_new(&self) -> bool {
        self.editing.as_ref().is_some_and(|e| e.item.is_none())
    }

    /// The text being typed holds `text` now: it ends with it.
    pub fn set_text(&mut self, text: String) {
        if let Some(editing) = &mut self.editing
            && editing.editor.text() != text
        {
            editing.editor = Editor::new(text);
        }
    }
}
