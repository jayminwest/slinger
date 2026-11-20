//! The pen's changes not yet written, kept on disk: what is kept and when
//! is [`super::editing::kept`]'s.

use super::Canvases;
use crate::backend::Unsent;

impl Canvases {
    /// Keeps the changes not yet written in `file` from now on, and first
    /// queues the changes it kept, to be sent before any made from now on.
    /// The file is this app's alone while it runs ([`Unsent::claim`]).
    pub fn keeping(mut self, file: Unsent) -> Self {
        self.editing.keeping(file, &mut self.follow);
        self
    }
}

#[cfg(test)]
mod tests;
