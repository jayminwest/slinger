//! `[sling]`: how a sling with no words of its own reads to the agent.

/// The words sent with a sling the user wrote no prompt for (`[sling]
/// prompt`): every one-tap sling, and the entry sent empty. The handwriting
/// is the question; these words tell the agent to read it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prompt(String);

impl Prompt {
    /// The words, never blank.
    pub fn text(&self) -> &str {
        &self.0
    }

    /// Any words but blank ones, trimmed.
    pub(super) fn parse(value: &str) -> Option<Prompt> {
        let value = value.trim();
        (!value.is_empty()).then(|| Prompt(value.to_string()))
    }
}

impl Default for Prompt {
    fn default() -> Self {
        Prompt("Read what I wrote and do what it asks.".to_string())
    }
}
