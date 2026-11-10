//! Replay: the lines of a file, applied in order to a new canvas.
//!
//! Ids in the file are authoritative. Before each `add` and `page.add`, the
//! canvas's next id is moved up to the one the file names, so lines this
//! reader skipped (a newer writer's items) never shift the ids after them.
//! This reaches into the canvas's id counters, which is why it lives inside
//! the `canvas` module.

use serde_json::Value;

use super::{Change, File, Line, json};
use crate::canvas::{Action, Canvas, Op};
use crate::{Error, Result};

impl File {
    /// The canvas the lines build, starting from [`Canvas::new`].
    ///
    /// An operation the canvas refuses (it names an item or page that is
    /// gone, an agent changes what it did not make) is a no-op, as it was
    /// when it was written (SPEC B4).
    ///
    /// # Errors
    ///
    /// [`Error::CanvasCorrupt`] when a line names an id at or below one the
    /// canvas already handed out: the file was changed outside slinger.
    pub fn canvas(&self) -> Result<Canvas> {
        let mut canvas = Canvas::new();
        for (i, line) in self.lines.iter().enumerate() {
            let at_line = i + 2;
            let Line::Op { by, at, change, .. } = line else {
                if let Line::Unknown { text, .. } = line {
                    reserve_unknown(&mut canvas, text);
                }
                continue;
            };
            let op = |action| Op { by: by.clone(), at: *at, action };
            match change {
                Change::Add { item, page, shape } => {
                    claim(&mut canvas.next_item, item.get(), at_line)?;
                    let action = Action::Add { page: *page, shape: shape.clone() };
                    canvas.apply(op(action)).ok();
                    canvas.next_item = item.get().saturating_add(1);
                }
                Change::PageAdd { page, name } => {
                    claim(&mut canvas.next_page, page.get(), at_line)?;
                    canvas.apply(op(Action::PageAdd { name: name.clone() })).ok();
                    canvas.next_page = page.get().saturating_add(1);
                }
                Change::PageRename { page, name } => {
                    let action = Action::PageRename { page: *page, name: name.clone() };
                    canvas.apply(op(action)).ok();
                }
                Change::PageRemove { page } => {
                    canvas.apply(op(Action::PageRemove { page: *page })).ok();
                }
                Change::Remove { items } => {
                    for item in items {
                        canvas.apply(op(Action::Remove { item: *item })).ok();
                    }
                }
                Change::Move { items, by: offset } => {
                    for item in items {
                        canvas.apply(op(Action::Move { item: *item, by: *offset })).ok();
                    }
                }
                Change::Restore { item } => {
                    canvas.apply(op(Action::Restore { item: item.clone() })).ok();
                }
                Change::PageRestore { page, index, items } => {
                    let action = Action::PageRestore {
                        page: page.clone(),
                        index: *index,
                        items: items.clone(),
                    };
                    canvas.apply(op(action)).ok();
                }
                Change::Set { item, fields } => {
                    let Some(current) = canvas.item(*item) else { continue };
                    let mut merged = json::fields(current.shape());
                    merged.extend(fields.iter().map(|(k, v)| (k.clone(), v.clone())));
                    if let Ok(Some(shape)) = json::read_shape(&merged) {
                        canvas.apply(op(Action::Edit { item: *item, shape })).ok();
                    }
                }
            }
        }
        Ok(canvas)
    }
}

/// Moves a counter up to `id`, the id the file says the next add made.
fn claim(next: &mut u64, id: u64, line: usize) -> Result<()> {
    if id < *next {
        return Err(Error::CanvasCorrupt {
            line,
            message: format!("id {id} was already handed out; ids must rise"),
        });
    }
    *next = id;
    Ok(())
}

/// An `add` of an item kind this reader does not know still used its id.
fn reserve_unknown(canvas: &mut Canvas, text: &str) {
    let Ok(Value::Object(m)) = serde_json::from_str::<Value>(text) else { return };
    if m.get("op").and_then(Value::as_str) != Some("add") {
        return;
    }
    let id = m.get("item").and_then(|i| i.get("id")).and_then(Value::as_str);
    if let Some(Ok(id)) = id.map(json::parse_item) {
        canvas.next_item = canvas.next_item.max(id.get().saturating_add(1));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canvas::Applied;

    #[test]
    fn an_add_applies_with_the_id_the_file_names() {
        let mut canvas = Canvas::new();
        claim(&mut canvas.next_item, 5, 2).unwrap();
        let page = canvas.pages().next().unwrap().id();
        let shape = crate::canvas::Shape::Box {
            frame: crate::canvas::Rect::default(),
            colour: crate::canvas::Colour::BLACK,
        };
        let op = Op {
            by: crate::canvas::Author::User,
            at: crate::canvas::Time(0),
            action: Action::Add { page, shape },
        };
        assert!(matches!(canvas.apply(op), Ok(Applied::Added(id)) if id.get() == 5));
        assert_eq!(claim(&mut canvas.next_item, 5, 9).unwrap_err().code(), "canvas_corrupt");
    }
}
