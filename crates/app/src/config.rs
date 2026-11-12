//! The app's config file. Pure: text in, a typed [`Config`] or an [`Error`]
//! with its line out. Reading, finding and watching the files is `backend`'s.
//!
//! The format is a small subset of TOML: `# comments`, `[table]` headers and
//! `key = "string"` lines. In `[keys]`, each key is a command's name and its
//! value is a chord (`commands`). Every key is optional; a missing key keeps its
//! default. `[pen] colours` and `[pen] widths` are lists in one string
//! ([`Palette`], [`Widths`]); `[canvas]` is the paper under the ink ([`Canvas`]);
//! `[sling] prompt` the words a sling with none sends ([`Prompt`]). A machine's overlay file is parsed the same way and merged over
//! the shared file key by key ([`Layer::over`]). `config.example.toml` at the
//! repository root lists every key with its default.

use std::fmt;
use std::path::{Path, PathBuf};

use crate::commands::{Chord, Command, Keymap};

pub mod canvas;
mod sling;
mod widths;

pub use canvas::Canvas;
pub use sling::Prompt;
pub use widths::{Width, Widths};

/// A colour, written `"#rrggbb"` in the file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

/// The one theme (SPEC P6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Theme {
    pub background: Rgb,
    pub text: Rgb,
    pub muted: Rgb,
    pub error: Rgb,
    /// The accent: the focus border (only with two or more panes), the caret,
    /// the selection, the sling field's edge and a working agent's mark.
    pub focus: Rgb,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            background: rgb(0x1e, 0x1e, 0x1e),
            text: rgb(0xe6, 0xe6, 0xe6),
            muted: rgb(0x8c, 0x8c, 0x8c),
            error: rgb(0xff, 0x6b, 0x6b),
            focus: rgb(0x4c, 0x9a, 0xff),
        }
    }
}

const fn rgb(r: u8, g: u8, b: u8) -> Rgb {
    Rgb { r, g, b }
}

/// The pen's colours, in the order the colour pop-up shows them: one to
/// [`Palette::MAX`], so each has a digit key. The first is the pen's colour
/// when the app starts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Palette(Vec<Rgb>);

impl Palette {
    /// The most colours a palette holds: the digit keys 1 to 9.
    pub const MAX: usize = 9;

    /// The colours, never none.
    pub fn colours(&self) -> &[Rgb] {
        &self.0
    }

    /// Parses `"#rrggbb #rrggbb"`: colours split by spaces or commas.
    fn parse(value: &str) -> Option<Palette> {
        let colours = value
            .split(|c: char| c == ',' || c.is_ascii_whitespace())
            .filter(|w| !w.is_empty())
            .map(parse_colour)
            .collect::<Option<Vec<_>>>()?;
        (1..=Self::MAX).contains(&colours.len()).then_some(Palette(colours))
    }
}

impl Default for Palette {
    /// The theme's text colour, then red, orange, yellow, green, blue, violet.
    fn default() -> Self {
        Palette(vec![
            Theme::default().text,
            rgb(0xff, 0x6b, 0x6b),
            rgb(0xff, 0xa9, 0x4d),
            rgb(0xff, 0xd4, 0x3b),
            rgb(0x69, 0xdb, 0x7c),
            rgb(0x4d, 0xab, 0xf7),
            rgb(0xb1, 0x97, 0xfc),
        ])
    }
}

/// Whether the sidebar is open when the app starts (`[window] sidebar`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Shown {
    #[default]
    Open,
    Closed,
}

/// Every setting, resolved: what the file says, else the default.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Config {
    pub theme: Theme,
    /// The daemon's socket as written (`~/` allowed; see [`expand_home`]);
    /// `None` is the daemon's default socket.
    pub socket: Option<String>,
    /// The workspace whose panes the window shows, by name; `None` is the
    /// first open workspace.
    pub workspace: Option<String>,
    pub keys: Keymap,
    pub palette: Palette,
    pub widths: Widths,
    pub sidebar: Shown,
    pub canvas: Canvas,
    pub prompt: Prompt,
}

/// What one file sets. Unset keys fall through to the layer below.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layer {
    background: Option<Rgb>,
    text: Option<Rgb>,
    muted: Option<Rgb>,
    error: Option<Rgb>,
    focus: Option<Rgb>,
    socket: Option<String>,
    workspace: Option<String>,
    palette: Option<Palette>,
    widths: Option<Widths>,
    sidebar: Option<Shown>,
    paper: Option<canvas::Background>,
    grid: Option<canvas::Grid>,
    prompt: Option<Prompt>,
    /// By command: `Some(None)` unbinds it.
    #[expect(
        clippy::option_option,
        reason = "unset (the layer below decides) and bound to no chord are different"
    )]
    keys: [Option<Option<Chord>>; Command::ALL.len()],
}

impl Default for Layer {
    /// Sets nothing. By hand: the standard library gives no `Default` for an
    /// array of more than 32 keys.
    fn default() -> Self {
        Self {
            background: None,
            text: None,
            muted: None,
            error: None,
            focus: None,
            socket: None,
            workspace: None,
            palette: None,
            widths: None,
            sidebar: None,
            paper: None,
            grid: None,
            prompt: None,
            keys: [None; Command::ALL.len()],
        }
    }
}

impl Layer {
    /// `top` merged over `self`: each key `top` sets wins, and a chord `top`
    /// binds is taken from the command `self` had on it.
    #[must_use]
    pub fn over(self, top: Layer) -> Layer {
        let mut keys = self.keys;
        for chord in top.keys.iter().filter(|k| matches!(k, Some(Some(_)))) {
            for key in &mut keys {
                if key == chord {
                    *key = Some(None);
                }
            }
        }
        for (key, top) in keys.iter_mut().zip(top.keys) {
            *key = top.or(*key);
        }
        Layer {
            background: top.background.or(self.background),
            text: top.text.or(self.text),
            muted: top.muted.or(self.muted),
            error: top.error.or(self.error),
            focus: top.focus.or(self.focus),
            socket: top.socket.or(self.socket),
            workspace: top.workspace.or(self.workspace),
            palette: top.palette.or(self.palette),
            widths: top.widths.or(self.widths),
            sidebar: top.sidebar.or(self.sidebar),
            paper: top.paper.or(self.paper),
            grid: top.grid.or(self.grid),
            prompt: top.prompt.or(self.prompt),
            keys,
        }
    }

    /// The config: each key this layer sets, and the default for the rest.
    pub fn resolve(self) -> Config {
        let d = Theme::default();
        let mut keys = Keymap::default();
        for (&(command, _), chord) in Command::ALL.iter().zip(self.keys) {
            if let Some(chord) = chord {
                keys.bind(command, chord);
            }
        }
        Config {
            theme: Theme {
                background: self.background.unwrap_or(d.background),
                text: self.text.unwrap_or(d.text),
                muted: self.muted.unwrap_or(d.muted),
                error: self.error.unwrap_or(d.error),
                focus: self.focus.unwrap_or(d.focus),
            },
            socket: self.socket,
            workspace: self.workspace,
            keys,
            palette: self.palette.unwrap_or_default(),
            widths: self.widths.unwrap_or_default(),
            sidebar: self.sidebar.unwrap_or_default(),
            canvas: Canvas {
                background: self.paper.unwrap_or_default(),
                grid: self.grid.unwrap_or_default(),
            },
            prompt: self.prompt.unwrap_or_default(),
        }
    }

    /// Sets one key from its unquoted string value. A key already set is an error.
    fn set(&mut self, table: &str, key: &str, value: String) -> Result<(), ErrorKind> {
        if table == "keys" {
            return self.bind(key, &value);
        }
        let slot = match (table, key) {
            ("daemon", "socket") => return put(&mut self.socket, value),
            ("workspace", "name") => return put(&mut self.workspace, value),
            ("pen", "colours") => {
                let palette = Palette::parse(&value).ok_or(ErrorKind::BadPalette)?;
                return put(&mut self.palette, palette);
            }
            ("pen", "widths") => {
                let widths = Widths::parse(&value).ok_or(ErrorKind::BadWidths)?;
                return put(&mut self.widths, widths);
            }
            ("window", "sidebar") => {
                let shown = match value.as_str() {
                    "open" => Shown::Open,
                    "closed" => Shown::Closed,
                    _ => return Err(ErrorKind::BadSidebar),
                };
                return put(&mut self.sidebar, shown);
            }
            ("canvas", "background") => {
                let paper = canvas::Background::parse(&value).ok_or(ErrorKind::BadBackground)?;
                return put(&mut self.paper, paper);
            }
            ("canvas", "grid") => {
                return put(&mut self.grid, canvas::Grid::parse(&value).ok_or(ErrorKind::BadGrid)?);
            }
            ("sling", "prompt") => {
                return put(&mut self.prompt, Prompt::parse(&value).ok_or(ErrorKind::BadPrompt)?);
            }
            ("theme", "background") => &mut self.background,
            ("theme", "text") => &mut self.text,
            ("theme", "muted") => &mut self.muted,
            ("theme", "error") => &mut self.error,
            ("theme", "focus") => &mut self.focus,
            _ => return Err(ErrorKind::UnknownKey),
        };
        put(slot, parse_colour(&value).ok_or(ErrorKind::BadColour)?)
    }

    /// Binds the command `name` to `chord`; `""` binds it to none.
    fn bind(&mut self, name: &str, chord: &str) -> Result<(), ErrorKind> {
        let command = Command::named(name).ok_or(ErrorKind::UnknownCommand)?;
        let chord = match chord {
            "" => None,
            text => Some(Chord::parse(text).ok_or(ErrorKind::BadChord)?),
        };
        let i = command as usize;
        if self.keys[i].is_none() && chord.is_some() && self.keys.contains(&Some(chord)) {
            return Err(ErrorKind::ChordTaken);
        }
        put(&mut self.keys[i], chord)
    }
}

/// Fills an unset `slot`; a set one means the key came twice.
fn put<T>(slot: &mut Option<T>, value: T) -> Result<(), ErrorKind> {
    if slot.is_some() {
        return Err(ErrorKind::DuplicateKey);
    }
    *slot = Some(value);
    Ok(())
}

const TABLES: [&str; 8] =
    ["theme", "daemon", "workspace", "pen", "window", "canvas", "sling", "keys"];

/// A line the parser rejects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Error {
    /// 1-based.
    pub line: usize,
    pub kind: ErrorKind,
}

/// Why a line is rejected. The line itself names the table, key or value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    /// Neither a comment, a `[table]` nor `key = "value"`.
    Syntax,
    UnknownTable,
    UnknownKey,
    DuplicateKey,
    /// The value is not one double-quoted string.
    NotAString,
    BadColour,
    /// Not one to nine colours.
    BadPalette,
    /// Not one to nine widths, each above 0 and at most 50.
    BadWidths,
    /// `[window] sidebar` is neither `open` nor `closed`.
    BadSidebar,
    /// `[canvas] background` is neither `plain` nor `dots`.
    BadBackground,
    /// `[canvas] grid` is not a distance from 1 to 500 points.
    BadGrid,
    /// `[sling] prompt` is blank.
    BadPrompt,
    /// A key in `[keys]` that names no command.
    UnknownCommand,
    BadChord,
    /// Two commands on one chord in one file.
    ChordTaken,
}

impl ErrorKind {
    /// What is wrong, and what to do about it.
    fn text(self) -> (&'static str, &'static str) {
        let every = "config.example.toml lists every table and key";
        match self {
            Self::Syntax => (
                "not a table, a setting or a comment",
                "write a [table] or key = \"value\"; start a comment with #",
            ),
            Self::UnknownTable => ("unknown table", every),
            Self::UnknownKey => ("unknown key", every),
            Self::DuplicateKey => {
                ("the key is set twice", "set each key once; put machine values in the overlay")
            }
            Self::NotAString => {
                ("the value is not a quoted string", "quote the value: key = \"value\"")
            }
            Self::BadColour => ("the value is not a colour", "write a colour as \"#rrggbb\""),
            Self::BadPalette => (
                "the value is not one to nine colours",
                "write the colours as \"#rrggbb #rrggbb\", split by spaces or commas",
            ),
            Self::BadWidths => (
                "the value is not one to nine widths from 0.1 to 50",
                "write the widths in points as \"2 4 8 16\", split by spaces or commas",
            ),
            Self::BadSidebar => ("the value is not open or closed", "write \"open\" or \"closed\""),
            Self::BadBackground => {
                ("the value is not plain or dots", "write \"plain\" or \"dots\"")
            }
            Self::BadGrid => (
                "the value is not a distance from 1 to 500 points",
                "write the canvas points between dots, with at most two decimal places: \"28.35\"",
            ),
            Self::BadPrompt => (
                "the value is blank",
                "write the words a sling with no prompt sends: \"Read what I wrote and do what it asks.\"",
            ),
            Self::UnknownCommand => ("unknown command", every),
            Self::BadChord => (
                "the value is not a key chord",
                "write a chord as \"cmd+shift+k\", or \"\" for none",
            ),
            Self::ChordTaken => {
                ("another command is on this chord", "bind each chord to one command")
            }
        }
    }
}

impl Error {
    /// What to do about it.
    pub fn hint(&self) -> &'static str {
        self.kind.text().1
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}: {}", self.line, self.kind.text().0)
    }
}

/// Parses one file. The first bad line is the error.
pub fn parse(text: &str) -> Result<Layer, Error> {
    let mut layer = Layer::default();
    let mut table = "";
    for (i, raw) in text.lines().enumerate() {
        let fail = |kind| Error { line: i + 1, kind };
        let line = raw.trim_ascii();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(header) = line.strip_prefix('[') {
            let (name, rest) = header.split_once(']').ok_or_else(|| fail(ErrorKind::Syntax))?;
            if !is_comment(rest) {
                return Err(fail(ErrorKind::Syntax));
            }
            table = name.trim_ascii();
            if !TABLES.contains(&table) {
                return Err(fail(ErrorKind::UnknownTable));
            }
            continue;
        }
        let (key, value) = line.split_once('=').ok_or_else(|| fail(ErrorKind::Syntax))?;
        let key = key.trim_ascii();
        if key.is_empty()
            || !key.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
        {
            return Err(fail(ErrorKind::Syntax));
        }
        let value = string(value).ok_or_else(|| fail(ErrorKind::NotAString))?;
        layer.set(table, key, value).map_err(fail)?;
    }
    Ok(layer)
}

/// Nothing but space and an optional comment.
fn is_comment(rest: &str) -> bool {
    let rest = rest.trim_ascii_start();
    rest.is_empty() || rest.starts_with('#')
}

/// One double-quoted string with `\"` and `\\` escapes, then an optional comment.
fn string(text: &str) -> Option<String> {
    let body = text.trim_ascii_start().strip_prefix('"')?;
    let mut chars = body.char_indices();
    let mut out = String::new();
    while let Some((i, c)) = chars.next() {
        match c {
            '"' => return is_comment(&body[i + 1..]).then_some(out),
            '\\' => match chars.next()? {
                (_, e @ ('"' | '\\')) => out.push(e),
                _ => return None,
            },
            c => out.push(c),
        }
    }
    None
}

fn parse_colour(value: &str) -> Option<Rgb> {
    let hex = value.strip_prefix('#').filter(|h| h.len() == 6)?;
    if !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let [_, r, g, b] = u32::from_str_radix(hex, 16).ok()?.to_be_bytes();
    Some(Rgb { r, g, b })
}

/// `path` with a leading `~/` replaced by `home`. Without a home, or without
/// the prefix, the path is used as written.
pub fn expand_home(path: &str, home: Option<&Path>) -> PathBuf {
    match (path.strip_prefix("~/"), home) {
        (Some(rest), Some(home)) => home.join(rest),
        _ => PathBuf::from(path),
    }
}

#[cfg(test)]
mod tests;
