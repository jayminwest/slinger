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
/// as `fig.name`, `fig.view` and the starting `fig.color`.
#[derive(Debug, Clone, PartialEq)]
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
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{}", place(*line, *col, message))]
pub struct ScriptError {
    pub line: u32,
    pub col: u32,
    pub message: String,
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
/// A [`ScriptError`] when the script is too long, the data too large, the
/// script throws (a bad argument to an API call fails at the call, by the
/// call's name), or a limit stops it.
pub fn run(script: &str, data: &Value, view: &View) -> Result<Drawn, ScriptError> {
    let whole = |message: String| ScriptError { line: 0, col: 0, message };
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
        "colours": colours,
        "items": ITEMS,
    });
    engine::run(script, &data, &input.to_string())
}

#[cfg(test)]
mod tests;
