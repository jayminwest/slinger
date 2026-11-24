//! Runs an agent's drawing script: JavaScript, in an embedded `QuickJS`,
//! bounded and pure (docs/design/figures.md, "The script API" and
//! Appendix C).
//!
//! One function, [`run`]: a script, its data and a read-only [`View`] in;
//! the shapes it drew and its warnings out, or a [`ScriptError`] with the
//! line and column where the script was. The API the script calls is a
//! JavaScript prelude over one native call that emits a shape or a warning
//! as JSON. Each shape is a `canvas apply` `add` item object (kind, colour,
//! the kind's fields and the figure keys; no `id`, no `page`), positions
//! and lengths in tenths relative to the figure's origin, so the core's own
//! strict item reader checks every shape and this crate links no core type.
//!
//! Pure: no clock the script can read, no timers, no console, no imports,
//! no IO, no `eval` of strings, and `Math.random` is seeded, so the same
//! script and data always give the same bytes out. Bounded: [`TIME`],
//! [`MEMORY`], [`STACK`], [`SCRIPT_BYTES`], [`DATA_BYTES`] and [`ITEMS`]
//! each stop a run with their own message.

mod colour;
mod engine;

use std::fmt;
use std::time::Duration;

use serde_json::Value;

pub use colour::{AGENT, COLOURS};

/// How long a run may take, prelude and checks included.
pub const TIME: Duration = Duration::from_secs(1);
/// The memory the engine may hold.
pub const MEMORY: usize = 32 * 1024 * 1024;
/// The stack the script may use.
pub const STACK: usize = 512 * 1024;
/// The longest script, in bytes.
pub const SCRIPT_BYTES: usize = 1024 * 1024;
/// The largest data, as JSON, in bytes.
pub const DATA_BYTES: usize = 256 * 1024;
/// The most shapes one figure holds.
pub const ITEMS: usize = 5_000;

/// What the script may read besides its data. Read-only: the script sees it
/// as `fig.name`, `fig.view`, the starting `fig.color` and the globals
/// `anchor` and `around` (docs/design/figures.md, rule 5).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct View {
    /// The figure's name; every shape carries it as `fig`.
    pub name: String,
    /// The `--over` or `--in` item's frame in canvas units, if any. The
    /// script sees its size, at its own origin: `{x: 0, y: 0, w, h}`.
    pub target: Option<[f64; 4]>,
    /// The agent colour as `#rrggbb`: the default colour of every shape.
    pub agent: String,
    /// The name of the page the figure goes on.
    pub page: String,
    /// The item the figure goes beside or over: the global `anchor`
    /// (`null` when none).
    pub anchor: Option<Anchor>,
    /// The bounds of the items around where the figure goes, with `--over`
    /// or `--at`, in figure coordinates, nearest first: the global
    /// `around` (`[]` when none).
    pub around: Vec<Near>,
}

/// The item a figure goes beside or over. The script sees
/// `{id, kind, w, h, text}` (`text` is `null` but for a text).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Anchor {
    /// Its id, or the figure address it was named by.
    pub id: String,
    /// Its kind (`text`, `box`, `stroke`, ...), or `figure` or `part`.
    pub kind: String,
    /// Its size in canvas units.
    pub w: f64,
    pub h: f64,
    /// Its words, for a text.
    pub text: Option<String>,
}

/// An item near the figure. The script sees `{id, kind, x, y, w, h}`, in
/// canvas units from the figure's origin.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Near {
    pub id: String,
    pub kind: String,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// What a script drew.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Drawn {
    /// `add` item objects, in the order the script drew them.
    pub items: Vec<Value>,
    /// What the script should know but that does not stop the draw.
    pub warnings: Vec<Warning>,
}

/// A note about a shape: text over a line, texts overlapping, a shape far
/// from the others, or the script's own `warn`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Warning {
    /// The script's line and column of the call, 1-based; 0 when unknown.
    pub line: u32,
    pub col: u32,
    pub message: String,
}

/// Why a script drew nothing. `line` and `col` are where the script was
/// (1-based), or 0 for a fault of the script as a whole (its size, its data).
/// It shows as one line, `script:L:C: message`; [`ScriptError::report`]
/// adds the source line and the stack.
#[derive(Debug, Clone, PartialEq, Eq, Default, thiserror::Error)]
#[error("{}", place(*line, *col, message))]
pub struct ScriptError {
    /// A fault of the script, or a limit it went over.
    pub kind: Fault,
    pub line: u32,
    pub col: u32,
    pub message: String,
    /// The script's line `line` as written (the source line; cut round
    /// `col` when it is long); empty with no line.
    pub shown: String,
    /// The calls the script was in, innermost first, each
    /// `name (script:L:C)`; only frames in the script.
    pub stack: Vec<String>,
}

/// What stopped a script: the two error codes, `script` and `limit`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Fault {
    /// A throw, a bad argument to an API call, a syntax error.
    #[default]
    Script,
    /// Over [`TIME`], [`MEMORY`], [`STACK`], [`SCRIPT_BYTES`],
    /// [`DATA_BYTES`] or [`ITEMS`].
    Limit,
}

/// The widest source line a report shows; a longer one is cut round the column.
const SOURCE_CHARS: usize = 120;

impl ScriptError {
    pub(crate) fn new(kind: Fault, (line, col): (u32, u32), message: String) -> Self {
        Self { kind, line, col, message, ..Self::default() }
    }

    /// The error for people and agents: the line `script:L:C: message`,
    /// then the source line with a mark under the column, then the stack
    /// when the script was inside a call of its own.
    #[must_use]
    pub fn report(&self) -> String {
        let mut out = vec![self.to_string()];
        if !self.shown.is_empty() {
            let number = self.line.to_string();
            out.push(format!("  {number} | {}", self.shown));
            if let Some(at) = self.mark() {
                out.push(format!("  {} | {}^", " ".repeat(number.len()), " ".repeat(at)));
            }
        }
        if self.stack.len() > 1 {
            out.extend(self.stack.iter().map(|frame| format!("  at {frame}")));
        }
        out.join("\n")
    }

    /// Where under the source line the column is, if the line was not cut.
    fn mark(&self) -> Option<usize> {
        let col = usize::try_from(self.col).ok()?.checked_sub(1)?;
        (!self.shown.starts_with('…') && col <= self.shown.chars().count()).then_some(col)
    }

    /// Fills `shown` from `script`'s line `line`.
    fn in_script(mut self, script: &str) -> Self {
        let Some(n) = usize::try_from(self.line).ok().and_then(|n| n.checked_sub(1)) else {
            return self;
        };
        let Some(text) = script.lines().nth(n) else { return self };
        let chars: Vec<char> = text.trim_end().chars().collect();
        self.shown = if chars.len() <= SOURCE_CHARS {
            chars.into_iter().collect()
        } else {
            let col = usize::try_from(self.col).unwrap_or(0).saturating_sub(1).min(chars.len());
            let start = col.saturating_sub(SOURCE_CHARS / 2).min(chars.len() - SOURCE_CHARS);
            let cut: String = chars[start..start + SOURCE_CHARS].iter().collect();
            let more = if start + SOURCE_CHARS < chars.len() { "…" } else { "" };
            format!("…{cut}{more}")
        };
        self
    }
}

impl fmt::Display for Warning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&place(self.line, self.col, &self.message))
    }
}

/// `script:12:7: message`, or `script: message` with no line.
fn place(line: u32, col: u32, message: &str) -> String {
    if line == 0 { format!("script: {message}") } else { format!("script:{line}:{col}: {message}") }
}

/// Runs `script` with `data` (the script's `fig.data`) and `view`.
///
/// # Errors
/// A [`ScriptError`]: a [`Fault::Script`] when the script throws (a bad
/// argument to an API call fails at the call, by the call's name); a
/// [`Fault::Limit`] when the script is too long, the data too large, or a
/// limit stops it.
pub fn run(script: &str, data: &Value, view: &View) -> Result<Drawn, ScriptError> {
    let whole = |message: String| ScriptError::new(Fault::Limit, (0, 0), message);
    if script.len() > SCRIPT_BYTES {
        return Err(whole(format!(
            "the script is {} bytes, more than {SCRIPT_BYTES} (1 MiB)",
            script.len()
        )));
    }
    let data = data.to_string();
    if data.len() > DATA_BYTES {
        return Err(whole(format!(
            "the data is {} bytes as JSON, more than {DATA_BYTES} (256 KiB)",
            data.len()
        )));
    }
    let colours: serde_json::Map<String, Value> =
        COLOURS.iter().map(|(name, hex)| ((*name).to_owned(), Value::from(*hex))).collect();
    let input = serde_json::json!({
        "name": view.name,
        "target": view.target,
        "agent": view.agent,
        "page": view.page,
        "anchor": view.anchor.as_ref().map(|a| serde_json::json!({
            "id": a.id, "kind": a.kind, "w": a.w, "h": a.h, "text": a.text,
        })),
        "around": view.around.iter().map(|n| serde_json::json!({
            "id": n.id, "kind": n.kind, "x": n.x, "y": n.y, "w": n.w, "h": n.h,
        })).collect::<Vec<_>>(),
        "colours": colours,
    });
    engine::run(script, &data, &input.to_string()).map_err(|e| e.in_script(script))
}

#[cfg(test)]
mod tests;
