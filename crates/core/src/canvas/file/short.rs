//! How long an agent's text may be. The user's column is the user's to
//! write in; an agent answers beside it with short texts, one beside each
//! subject, so a reply never buries what it answers. The rule is checked when
//! an operation is first written ([`Log::step`]), never on replay: a canvas
//! that holds an older, longer text still opens, and moving or recolouring
//! such a text still works.

use super::Log;
use crate::canvas::{Action, Author, Shape};
use crate::{Error, Result, render};

/// The most rows an agent's text may have at its column.
pub const AGENT_ROWS: usize = 12;

impl Log {
    /// Fails with [`Error::TooLong`] when `by` is an agent and an action
    /// makes a text of more than [`AGENT_ROWS`] rows, or edits one to more
    /// rows than it had and more than [`AGENT_ROWS`]. The user's texts have
    /// no such limit.
    pub(super) fn short(&self, by: &Author, actions: &[Action]) -> Result<()> {
        if *by == Author::User {
            return Ok(());
        }
        for action in actions {
            let (shape, before) = match action {
                Action::Add { shape, .. } => (shape, 0),
                Action::Edit { item, shape } => {
                    (shape, self.canvas().item(*item).map_or(0, |i| rows(i.shape())))
                }
                _ => continue,
            };
            let rows = rows(shape);
            if rows > AGENT_ROWS && rows > before {
                return Err(Error::TooLong {
                    message: format!("{by}'s text is {rows} rows, more than {AGENT_ROWS}"),
                    hint: Some("split it: one short put beside each subject".into()),
                });
            }
        }
        Ok(())
    }
}

/// The rows a text takes at its column; any other shape takes none.
fn rows(shape: &Shape) -> usize {
    match shape {
        Shape::Text { text, frame, size, .. } => render::text_rows(text, frame.w, *size),
        _ => 0,
    }
}
