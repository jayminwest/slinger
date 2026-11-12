//! The clipboard, when no text field takes cut, copy or paste ([`Edit`]):
//! what it holds for the app ([`Clip`]), and pasted text (cmd+V, and
//! Flick's dictation, which pastes what it heard) made fit to go where
//! typed text goes. Pure: `platform` reads and writes the clipboard and
//! hands a [`Clip`] in or takes one back; each text that takes typing (a
//! canvas text, the palette's search) takes it as its own characters, not
//! as keys, so a pasted delete or escape is not a key press. A canvas's
//! items go as their file's objects ([`crate::mirror::clip`]).

/// What the clipboard holds for the app, or what a copy puts there: plain
/// text, and a canvas's items (as [`crate::mirror::clip`] writes them).
/// Neither is ever empty.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Clip {
    pub text: Option<String>,
    pub items: Option<String>,
}

impl Clip {
    /// A clip of `text` and `items`, each left out when empty.
    pub fn new(text: Option<String>, items: Option<String>) -> Self {
        let full = |s: Option<String>| s.filter(|s| !s.is_empty());
        Self { text: full(text), items: full(items) }
    }

    /// Whether it holds nothing.
    pub fn is_empty(&self) -> bool {
        self.text.is_none() && self.items.is_none()
    }
}

/// An edit of the Edit menu (or its key) that no text field took: the app
/// does it, or asks whether it can, to grey the item when not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Edit {
    Cut,
    Copy,
    /// Paste what the clipboard holds.
    Paste(Clip),
}

/// `text` with each line end (`\r\n`, `\r` or `\n`) as `\n`, each tab as a
/// space and every other control or function-key character gone.
pub fn plain(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\r' => {
                chars.next_if_eq(&'\n');
                out.push('\n');
            }
            '\n' => out.push('\n'),
            '\t' => out.push(' '),
            // The arrow and function keys are in this private range.
            c if c.is_control() || ('\u{f700}'..='\u{f8ff}').contains(&c) => {}
            c => out.push(c),
        }
    }
    out
}

/// `text` as one line: [`plain`], with each line end a space.
pub fn one_line(text: &str) -> String {
    plain(text).replace('\n', " ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_ends_become_newlines_and_other_controls_go() {
        assert_eq!(plain("one\r\ntwo\rthree\nfour"), "one\ntwo\nthree\nfour");
        assert_eq!(plain("a\tb"), "a b");
        assert_eq!(plain("x\u{1b}\u{7f}\u{8}\u{f700}y"), "xy", "no key press hides in a paste");
        assert_eq!(plain("héllo — ✓"), "héllo — ✓");
        assert_eq!(plain(""), "");
    }

    #[test]
    fn a_clip_leaves_out_what_is_empty() {
        let clip = Clip::new(Some(String::new()), Some("[]".into()));
        assert_eq!(clip, Clip { text: None, items: Some("[]".into()) });
        assert!(Clip::new(None, Some(String::new())).is_empty());
    }

    #[test]
    fn one_line_joins_the_lines_with_spaces() {
        assert_eq!(one_line("ideas\r\nfor monday\n"), "ideas for monday ");
    }
}
