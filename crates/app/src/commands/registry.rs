//! The command registry (SPEC D21): the groups commands show under, and a
//! chord printed as macOS prints it. Each command's title and group are its
//! row of the command table (`table`). The key overlay, the command palette
//! and the menu bar read these and the live [`Keymap`](super::Keymap), so a
//! command added to the table shows in all three. Pure.

use super::{ALT, CMD, CTRL, Chord, DOWN, F1, LEFT, RIGHT, SHIFT, UP};

/// Where a command shows: a heading in the key overlay, a menu in the menu bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Group {
    Tools,
    PopUps,
    Sling,
    File,
    Canvas,
    Pages,
    Panes,
    Window,
}

impl Group {
    /// Every group, in the order the surfaces show them.
    pub const ALL: [Group; 8] = [
        Group::Tools,
        Group::PopUps,
        Group::Sling,
        Group::File,
        Group::Canvas,
        Group::Pages,
        Group::Panes,
        Group::Window,
    ];

    /// The group's heading: "Pop-ups".
    pub const fn title(self) -> &'static str {
        match self {
            Group::Tools => "Tools",
            Group::PopUps => "Pop-ups",
            Group::Sling => "Sling",
            Group::File => "File",
            Group::Canvas => "Canvas",
            Group::Pages => "Pages",
            Group::Panes => "Panes",
            Group::Window => "Window",
        }
    }
}

/// Each modifier's glyph, in the order macOS prints them.
pub(super) const MOD_GLYPHS: [(u8, char); 4] = [(CTRL, '⌃'), (ALT, '⌥'), (SHIFT, '⇧'), (CMD, '⌘')];

/// The glyph of each key that does not print as itself.
pub(super) const KEY_GLYPHS: [(char, &str); 10] = [
    (' ', "Space"),
    ('\t', "⇥"),
    ('\r', "↩"),
    ('\u{1b}', "⎋"),
    ('\u{7f}', "⌫"),
    ('\u{f728}', "⌦"),
    (UP, "↑"),
    (DOWN, "↓"),
    (LEFT, "←"),
    (RIGHT, "→"),
];

impl Chord {
    /// The chord as macOS prints it: `cmd+shift+d` is "⇧⌘D", `cmd+return`
    /// is "⌘↩", `f5` is "F5".
    pub fn glyphs(self) -> String {
        let mut out: String =
            MOD_GLYPHS.iter().filter(|&&(bit, _)| self.mods & bit != 0).map(|&(_, g)| g).collect();
        if let Some(&(_, glyph)) = KEY_GLYPHS.iter().find(|&&(c, _)| c == self.key) {
            out.push_str(glyph);
        } else if let Some(n) = function_key(self.key) {
            out.push('F');
            out.push_str(&n.to_string());
        } else {
            out.push(self.key.to_ascii_uppercase());
        }
        out
    }
}

/// `n` for the key `fn`, 1 to 20.
fn function_key(key: char) -> Option<u8> {
    let n = u32::from(key).checked_sub(F1)?;
    u8::try_from(n + 1).ok().filter(|&n| n <= 20)
}
