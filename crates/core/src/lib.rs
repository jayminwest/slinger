//! One window onto your computers: think with a pen on a canvas, sling anything to an agent, and get things back instead of chat.
//!
//! The core holds the domain logic. It does no terminal or process IO and knows
//! nothing about its front ends, so every consumer gets the same behavior and
//! the same errors.

pub mod canvas;
mod error;
pub mod feedback;
pub mod packet;
pub mod record;
pub mod render;
pub mod session;
pub mod sling;
pub mod tldraw;
pub mod ui;
pub mod workspace;

pub use error::{Error, Result};
