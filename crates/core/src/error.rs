//! Typed errors. Every failure the core can report has a stable code, so front
//! ends map it to an exit code or a status without parsing messages. Add a
//! variant only when a caller needs to tell it apart from the others.

/// Result alias for the core.
pub type Result<T, E = Error> = std::result::Result<T, E>;

/// A failure a caller can act on.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// A named thing does not exist.
    #[error("{what} not found")]
    NotFound { what: String, hint: Option<String> },
    /// The thing exists or is in the wrong state for the request.
    #[error("{message}")]
    Conflict { message: String, hint: Option<String> },
    /// The input was understood but rejected.
    #[error("{message}")]
    Invalid { message: String, hint: Option<String> },
    /// An agent's text is longer than an agent may write in one item.
    #[error("{message}")]
    TooLong { message: String, hint: Option<String> },
    /// A drawing script failed: a throw, a bad argument to an API call, a
    /// syntax error. The message names the line and column and shows the
    /// source line and the calls the script was in.
    #[error("{message}")]
    Script { message: String },
    /// A drawing script went over a limit: its time, memory, stack, size,
    /// data or shapes. The message says which, and where.
    #[error("{message}")]
    Limit { message: String },
    /// A canvas file is damaged before its last line: it was changed outside
    /// slinger. `line` counts from 1.
    #[error("the canvas file is damaged at line {line}: {message}")]
    CanvasCorrupt { line: usize, message: String },
    /// A canvas file was written in a format version this reader does not know.
    #[error("the canvas file is format version {version}, newer than this slinger reads")]
    CanvasTooNew { version: u64 },
    /// A packet was written in a format version this reader does not know.
    #[error("the packet is format version {version}, which this slinger does not read")]
    UnsupportedVersion { version: String },
}

impl Error {
    /// Stable machine-readable code. Never rename one.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::NotFound { .. } => "not_found",
            Self::Conflict { .. } => "conflict",
            Self::Invalid { .. } => "invalid",
            Self::TooLong { .. } => "too_long",
            Self::Script { .. } => "script",
            Self::Limit { .. } => "limit",
            Self::CanvasCorrupt { .. } => "canvas_corrupt",
            Self::CanvasTooNew { .. } => "canvas_too_new",
            Self::UnsupportedVersion { .. } => "unsupported_version",
        }
    }

    /// What the caller can do about it, if anything.
    #[must_use]
    pub fn hint(&self) -> Option<&str> {
        match self {
            Self::NotFound { hint, .. }
            | Self::Conflict { hint, .. }
            | Self::Invalid { hint, .. }
            | Self::TooLong { hint, .. } => hint.as_deref(),
            Self::Script { .. } => Some(
                "fix the script there and try it with --dry-run; 'slinger guide' has the drawing API",
            ),
            Self::Limit { .. } => Some(
                "do less in the script, or draw it as two figures; 'slinger guide' lists the limits",
            ),
            Self::CanvasCorrupt { .. } => {
                Some("the file was changed outside slinger; restore it from a backup")
            }
            Self::CanvasTooNew { .. } => Some("upgrade slinger to open this canvas"),
            Self::UnsupportedVersion { .. } => Some("read it with the slinger that wrote it"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_are_stable() {
        let hint = None;
        assert_eq!(Error::NotFound { what: "x".into(), hint: hint.clone() }.code(), "not_found");
        assert_eq!(Error::Conflict { message: "x".into(), hint: hint.clone() }.code(), "conflict");
        assert_eq!(Error::Invalid { message: "x".into(), hint: hint.clone() }.code(), "invalid");
        assert_eq!(Error::TooLong { message: "x".into(), hint }.code(), "too_long");
        assert_eq!(Error::Script { message: "x".into() }.code(), "script");
        assert_eq!(Error::Limit { message: "x".into() }.code(), "limit");
    }

    #[test]
    fn not_found_names_the_thing() {
        let err = Error::NotFound { what: "project ada".into(), hint: None };
        assert_eq!(err.to_string(), "project ada not found");
        assert_eq!(err.hint(), None);
    }
}
