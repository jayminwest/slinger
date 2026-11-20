//! What a mouse or trackpad press does with the pen tool (`[pointer]`,
//! docs/design/pen-less.md pick 1B). Pure, like the rest of `config`.

/// What a mouse or trackpad press does with the pen tool chosen
/// (`[pointer] press`). A tablet pen always does what the chosen tool does.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Press {
    /// It selects and moves, as the select tool does: a mouse draws no
    /// stray line.
    #[default]
    Select,
    /// It does what the chosen tool does: the mouse draws with the pen.
    Tool,
}

impl Press {
    /// Parses `"select"` or `"tool"`.
    pub(super) fn parse(value: &str) -> Option<Press> {
        match value {
            "select" => Some(Press::Select),
            "tool" => Some(Press::Tool),
            _ => None,
        }
    }
}
