//! The app's config file. Pure: text in, a typed [`Config`] or an [`Error`]
//! with its line out. Reading, finding and watching the files is `backend`'s.
//!
//! The format is a small subset of TOML: `# comments`, `[table]` headers and
//! `key = "string"` lines. Every key is optional; a missing key keeps its
//! default. A machine's overlay file is parsed the same way and merged over
//! the shared file key by key ([`Layer::over`]). `config.example.toml` at the
//! repository root lists every key with its default.

use std::fmt;
use std::path::{Path, PathBuf};

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
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            background: rgb(0x1e, 0x1e, 0x1e),
            text: rgb(0xe6, 0xe6, 0xe6),
            muted: rgb(0x8c, 0x8c, 0x8c),
            error: rgb(0xff, 0x6b, 0x6b),
        }
    }
}

const fn rgb(r: u8, g: u8, b: u8) -> Rgb {
    Rgb { r, g, b }
}

/// Every setting, resolved: what the file says, else the default.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Config {
    pub theme: Theme,
    /// The daemon's socket as written (`~/` allowed; see [`expand_home`]);
    /// `None` is the daemon's default socket.
    pub socket: Option<String>,
}

/// What one file sets. Unset keys fall through to the layer below.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Layer {
    background: Option<Rgb>,
    text: Option<Rgb>,
    muted: Option<Rgb>,
    error: Option<Rgb>,
    socket: Option<String>,
}

impl Layer {
    /// `top` merged over `self`: each key `top` sets wins.
    #[must_use]
    pub fn over(self, top: Layer) -> Layer {
        Layer {
            background: top.background.or(self.background),
            text: top.text.or(self.text),
            muted: top.muted.or(self.muted),
            error: top.error.or(self.error),
            socket: top.socket.or(self.socket),
        }
    }

    /// The config: each key this layer sets, and the default for the rest.
    pub fn resolve(self) -> Config {
        let d = Theme::default();
        Config {
            theme: Theme {
                background: self.background.unwrap_or(d.background),
                text: self.text.unwrap_or(d.text),
                muted: self.muted.unwrap_or(d.muted),
                error: self.error.unwrap_or(d.error),
            },
            socket: self.socket,
        }
    }

    /// Sets one key from its unquoted string value. A key already set is an error.
    fn set(&mut self, table: &str, key: &str, value: String) -> Result<(), ErrorKind> {
        let slot = match (table, key) {
            ("daemon", "socket") => return put(&mut self.socket, value),
            ("theme", "background") => &mut self.background,
            ("theme", "text") => &mut self.text,
            ("theme", "muted") => &mut self.muted,
            ("theme", "error") => &mut self.error,
            _ => return Err(ErrorKind::UnknownKey),
        };
        put(slot, parse_colour(&value).ok_or(ErrorKind::BadColour)?)
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

const TABLES: [&str; 2] = ["theme", "daemon"];

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
