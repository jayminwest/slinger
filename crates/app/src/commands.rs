//! Key commands (SPEC P4): every action the app takes from the keyboard is a
//! named [`Command`], bound to a [`Chord`] in the config file. A tablet button
//! is a key, set in the tablet's own software. Pure: `platform` turns a key
//! event into a chord, and a [`Keymap`] turns the chord into a command.

/// Every action a key can run. Later seeds add to [`Command::ALL`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    Quit,
    ReloadConfig,
    /// Open the colour pop-up at the pen.
    PickColour,
    /// Open the shape pop-up (box, arrow) at the pen.
    PickShape,
    /// Open the tool pop-up (pen, eraser, lasso, text) at the pen.
    PickTool,
    /// Split the focused pane; the new pane, on the right, shows the same.
    SplitRight,
    /// Split the focused pane; the new pane, below, shows the same.
    SplitDown,
    /// Close the focused pane. The last pane stays.
    ClosePane,
    FocusNext,
    FocusLeft,
    FocusRight,
    FocusUp,
    FocusDown,
    /// Give the focused pane more of its split.
    GrowPane,
    ShrinkPane,
}

/// How many commands there are.
const COUNT: usize = Command::ALL.len();

impl Command {
    /// Each command and its name in the config file, in declaration order.
    pub const ALL: [(Command, &str); 15] = [
        (Command::Quit, "quit"),
        (Command::ReloadConfig, "reload-config"),
        (Command::PickColour, "pick-colour"),
        (Command::PickShape, "pick-shape"),
        (Command::PickTool, "pick-tool"),
        (Command::SplitRight, "split-right"),
        (Command::SplitDown, "split-down"),
        (Command::ClosePane, "close-pane"),
        (Command::FocusNext, "focus-next"),
        (Command::FocusLeft, "focus-left"),
        (Command::FocusRight, "focus-right"),
        (Command::FocusUp, "focus-up"),
        (Command::FocusDown, "focus-down"),
        (Command::GrowPane, "grow-pane"),
        (Command::ShrinkPane, "shrink-pane"),
    ];

    /// The command called `name` in the config file.
    pub fn named(name: &str) -> Option<Command> {
        Self::ALL.iter().find(|(_, n)| *n == name).map(|&(c, _)| c)
    }
}

/// The modifier bits of a [`Chord`].
pub const SHIFT: u8 = 1;
pub const CTRL: u8 = 2;
pub const ALT: u8 = 4;
pub const CMD: u8 = 8;

/// A key with its modifiers: `"cmd+shift+k"` in the config file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Chord {
    mods: u8,
    key: char,
}

/// The names of the keys that do not type a character, and the code point
/// macOS gives each. `f1` to `f20` are parsed apart.
const NAMED: [(&str, char); 10] = [
    ("space", ' '),
    ("tab", '\t'),
    ("return", '\r'),
    ("escape", '\u{1b}'),
    ("delete", '\u{7f}'),
    ("up", UP),
    ("down", DOWN),
    ("left", LEFT),
    ("right", RIGHT),
    ("forward-delete", '\u{f728}'),
];

const UP: char = '\u{f700}';
const DOWN: char = '\u{f701}';
const LEFT: char = '\u{f702}';
const RIGHT: char = '\u{f703}';

/// `f1`'s code point; `f2` to `f20` follow it.
const F1: u32 = 0xf704;

impl Chord {
    /// A key event: `mods` is a set of the bits above (others are dropped),
    /// and `key` is the character the key types with no modifier but shift.
    /// A letter matches in either case.
    pub const fn new(mods: u8, key: char) -> Chord {
        Chord { mods: mods & (SHIFT | CTRL | ALT | CMD), key: key.to_ascii_lowercase() }
    }

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
fn key_named(name: &str) -> Option<char> {
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

/// Which chord runs each command. A command may have none; a chord runs at
/// most one command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Keymap([Option<Chord>; COUNT]);

impl Default for Keymap {
    /// The few defaults; `config.example.toml` lists them. The final keys
    /// wait for a canvas to use them on (SPEC O4).
    fn default() -> Self {
        let on = |mods, key| Some(Chord { mods, key });
        Keymap([
            on(CMD, 'q'),
            on(CMD, 'r'),
            on(CMD | SHIFT, 'c'),
            on(CMD | SHIFT, 's'),
            on(CMD | SHIFT, 't'),
            on(CMD, 'd'),
            on(CMD | SHIFT, 'd'),
            on(CMD, 'w'),
            on(CTRL, '\t'),
            on(CMD | ALT, LEFT),
            on(CMD | ALT, RIGHT),
            on(CMD | ALT, UP),
            on(CMD | ALT, DOWN),
            on(CMD, '='),
            on(CMD, '-'),
        ])
    }
}

impl Keymap {
    /// The command `chord` runs, if any.
    pub fn command(&self, chord: Chord) -> Option<Command> {
        let i = self.0.iter().position(|&c| c == Some(chord))?;
        Some(Command::ALL[i].0)
    }

    /// Puts `command` on `chord`, or on none. A command that was on `chord`
    /// loses it.
    pub fn bind(&mut self, command: Command, chord: Option<Chord>) {
        if chord.is_some() {
            for slot in &mut self.0 {
                if *slot == chord {
                    *slot = None;
                }
            }
        }
        self.0[command as usize] = chord;
    }
}

#[cfg(test)]
mod tests;
