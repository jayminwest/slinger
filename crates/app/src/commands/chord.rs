//! Chords in the config file's words: the names of the keys that type no
//! character, and [`Chord::parse`].

use super::{ALT, CMD, CTRL, Chord, SHIFT};

/// The names of the keys that do not type a character, and the code point
/// macOS gives each. `f1` to `f20` are parsed apart.
const NAMED: [(&str, char); 10] = [
    ("space", ' '),
    ("tab", '\t'),
    ("return", '\r'),
    ("escape", '\u{1b}'),
    ("delete", DELETE),
    ("up", UP),
    ("down", DOWN),
    ("left", LEFT),
    ("right", RIGHT),
    ("forward-delete", FORWARD_DELETE),
];

/// The code points macOS gives the keys that move a caret, and delete.
pub const UP: char = '\u{f700}';
pub const DOWN: char = '\u{f701}';
pub const LEFT: char = '\u{f702}';
pub const RIGHT: char = '\u{f703}';
pub const FORWARD_DELETE: char = '\u{f728}';
pub const HOME: char = '\u{f729}';
pub const END: char = '\u{f72b}';
pub const DELETE: char = '\u{7f}';

/// `f1`'s code point; `f2` to `f20` follow it.
pub(super) const F1: u32 = 0xf704;

impl Chord {
    /// Parses `"cmd+q"`: lowercase modifiers (`shift`, `ctrl`, `alt`, `cmd`)
    /// joined by `+`, then one character or a key's name (`space`, `f5`).
    /// `"cmd++"` is the plus key. Each modifier comes at most once.
    pub fn parse(text: &str) -> Option<Chord> {
        let (mods, key) = match text.strip_suffix('+') {
            Some("") => ("", "+"),
            Some(rest) => (rest.strip_suffix('+')?, "+"),
            None => text.rsplit_once('+').unwrap_or(("", text)),
        };
        let mut bits = 0;
        for name in mods.split('+').filter(|_| !mods.is_empty()) {
            let bit = match name {
                "shift" => SHIFT,
                "ctrl" => CTRL,
                "alt" => ALT,
                "cmd" => CMD,
                _ => return None,
            };
            if bits & bit != 0 {
                return None;
            }
            bits |= bit;
        }
        Some(Chord::new(bits, key_named(key)?))
    }
}

/// One character, or the code point of a named key.
pub(super) fn key_named(name: &str) -> Option<char> {
    let mut chars = name.chars();
    if let (Some(c), None) = (chars.next(), chars.next()) {
        return (!c.is_control() && c != ' ').then_some(c);
    }
    if let Some(&(_, c)) = NAMED.iter().find(|(n, _)| *n == name) {
        return Some(c);
    }
    // `f1` to `f20`, by hand: `str::parse` costs the app binary its size budget.
    let n = match name.strip_prefix('f')?.as_bytes() {
        &[d @ b'1'..=b'9'] => d - b'0',
        &[b'1', d @ b'0'..=b'9'] => 10 + d - b'0',
        b"20" => 20,
        _ => return None,
    };
    char::from_u32(F1 + u32::from(n) - 1)
}
