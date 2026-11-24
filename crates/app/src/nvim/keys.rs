//! Keys in Nvim's notation (`:help keycodes`), for `nvim_input`. Pure.
//!
//! A key press reaches a text being edited twice over: first as its
//! [`Chord`], then, when that was passed on, as the text it types. So the
//! keys that type no character and every control chord go as the chord
//! ([`chord`]); the rest go as their text ([`text`]), which carries the
//! layout, shift and option as macOS composed them. Command chords are
//! never Nvim's: they stay the app's.

use std::fmt::Write as _;

use crate::commands::{
    ALT, CMD, CTRL, Chord, DELETE, DOWN, END, FORWARD_DELETE, HOME, LEFT, RIGHT, SHIFT, UP,
};

/// The names of the keys that type no character.
const NAMED: [(char, &str); 14] = [
    ('\u{1b}', "Esc"),
    ('\r', "CR"),
    ('\t', "Tab"),
    (DELETE, "BS"),
    (FORWARD_DELETE, "Del"),
    (UP, "Up"),
    (DOWN, "Down"),
    (LEFT, "Left"),
    (RIGHT, "Right"),
    (HOME, "Home"),
    (END, "End"),
    ('\u{f72c}', "PageUp"),
    ('\u{f72d}', "PageDown"),
    ('\u{3}', "CR"),
];

/// `chord` in Nvim's notation, when it goes as a chord: a named key with
/// any modifiers but command, or control with a character. `None` for a
/// command chord (the app's) and for a key that types (its text goes).
pub fn chord(chord: Chord) -> Option<String> {
    let mods = chord.mods();
    if mods & CMD != 0 {
        return None;
    }
    let key = chord.key();
    let named = NAMED.iter().find(|(c, _)| *c == key).map(|(_, name)| (*name).to_string());
    let function = || {
        let n = u32::from(key).checked_sub(0xf704).filter(|n| *n < 20)?;
        Some(format!("F{}", n + 1))
    };
    let name = match named.or_else(function) {
        Some(name) => name,
        None if mods & CTRL != 0 && !key.is_control() => match key {
            '<' => "lt".to_string(),
            ' ' => "Space".to_string(),
            key => key.to_string(),
        },
        None => return None,
    };
    let mut out = String::from("<");
    for (bit, prefix) in [(CTRL, "C-"), (SHIFT, "S-"), (ALT, "M-")] {
        if mods & bit != 0 {
            out.push_str(prefix);
        }
    }
    out.push_str(&name);
    out.push('>');
    Some(out)
}

/// Typed `typed` in Nvim's notation: `<` is `<lt>`, and a control
/// character a key typed is that key.
pub fn text(typed: &str) -> String {
    let mut out = String::new();
    for c in typed.chars() {
        match c {
            '<' => out.push_str("<lt>"),
            c if NAMED.iter().any(|(key, _)| *key == c) => {
                let name = NAMED.iter().find(|(key, _)| *key == c).map_or("", |(_, name)| name);
                let _ = write!(out, "<{name}>");
            }
            // Function keys' private code points type nothing.
            c if ('\u{f700}'..='\u{f8ff}').contains(&c) => {}
            c if c.is_control() => {}
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn named_keys_and_control_chords_go_as_chords_and_the_rest_as_text() {
        let key = |text: &str| Chord::parse(text).and_then(chord);
        assert_eq!(key("escape").as_deref(), Some("<Esc>"));
        assert_eq!(key("ctrl+r").as_deref(), Some("<C-r>"));
        assert_eq!(key("shift+tab").as_deref(), Some("<S-Tab>"));
        assert_eq!(key("alt+left").as_deref(), Some("<M-Left>"));
        assert_eq!(key("delete").as_deref(), Some("<BS>"));
        assert_eq!(key("f5").as_deref(), Some("<F5>"));
        assert_eq!(
            (key("a"), key("shift+a"), key("alt+a"), key("cmd+z")),
            (None, None, None, None)
        );
        assert_eq!(text("a<b\r"), "a<lt>b<CR>");
    }
}
