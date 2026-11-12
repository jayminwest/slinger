//! Pasted text (cmd+V, and Flick's dictation, which pastes what it heard):
//! the clipboard's plain text, made fit to go where typed text goes. Pure:
//! `platform` reads the clipboard and hands the text in; each text that
//! takes typing (a canvas text, a page's new name, the palette's search)
//! takes it as its own characters, not as keys, so a pasted delete or
//! escape is not a key press.

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
    fn one_line_joins_the_lines_with_spaces() {
        assert_eq!(one_line("ideas\r\nfor monday\n"), "ideas for monday ");
    }
}
