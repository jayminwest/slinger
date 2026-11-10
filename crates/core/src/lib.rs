//! One window onto your computers: think with a pen on a canvas, sling anything to an agent, and get things back instead of chat.
//!
//! The core holds the domain logic. It does no terminal or process IO and knows
//! nothing about its front ends, so every consumer gets the same behavior and
//! the same errors.

pub mod canvas;
mod error;
pub mod packet;
pub mod record;
pub mod render;
pub mod session;
pub mod sling;
pub mod tldraw;
pub mod ui;
pub mod workspace;

pub use error::{Error, Result};

/// Placeholder capability: replace it with the first real one.
///
/// # Errors
///
/// [`Error::Invalid`] when `name` is blank.
pub fn greet(name: &str) -> Result<String> {
    let name = name.trim();
    if name.is_empty() {
        return Err(Error::Invalid {
            message: "name is empty".into(),
            hint: Some("pass a non-empty name".into()),
        });
    }
    Ok(format!("hello, {name}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn greets_a_trimmed_name() {
        assert_eq!(greet("  ada ").unwrap(), "hello, ada");
    }

    #[test]
    fn rejects_a_blank_name_as_invalid() {
        let err = greet("   ").unwrap_err();
        assert_eq!(err.code(), "invalid");
        assert!(err.hint().is_some());
    }
}
