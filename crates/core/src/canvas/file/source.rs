//! What a redraw needs, kept in the canvas file (docs/design/figures.md,
//! "Where a figure's script and data live"): a draw that writes appends one
//! `figure` line after its step, with the script whole, the data whole, the
//! origin and frame it placed the figure at, the frame it drew over or in
//! and the item it drew beside or over. The figure's last `figure` line is its [`Source`], so a canvas file
//! copied to another machine or data directory redraws as it did here. The
//! line is no undo step and changes no item; a reader from before it skips
//! it (B5) and loses only the redraw.

use serde_json::Value;

use super::json::{Fields, Obj};
use super::{Change, Line, Log, Written};
use crate::canvas::{Author, Step, Time};

/// A figure's script as a draw runs it: its text, its data, the frame it
/// draws over or in (`[x, y, w, h]` in tenths) and the item it draws
/// beside or over (its id or address, as the draw named it).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Script {
    pub text: String,
    pub data: Value,
    pub target: Option<[i64; 4]>,
    pub anchor: Option<String>,
}

/// A figure's last draw, as its canvas file keeps it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
    /// The script, its data and the frame it drew over or in.
    pub script: Script,
    /// Where its (0,0) went, `[x, y]` in tenths.
    pub origin: [i64; 2],
    /// The joint frame of its items as placed, `[x, y, w, h]` in tenths.
    pub frame: [i64; 4],
}

impl Log {
    /// The last draw of the figure `name`, if a script drew it.
    #[must_use]
    pub fn source(&self, name: &str) -> Option<&Source> {
        self.sources.get(name)
    }

    /// Keeps `source` as the figure `name`'s last draw by `by` at `at`:
    /// its `figure` line goes after what `written` writes.
    pub(super) fn keep(
        &mut self,
        (by, at): (&Author, Time),
        name: &str,
        source: Source,
        written: &mut Written,
    ) {
        let change = Change::Figure { name: name.to_owned(), source: source.clone() };
        let line = Line::Op { n: self.next(), by: by.clone(), at, change, step: Step::None };
        self.push(&line, written);
        self.sources.insert(name.to_owned(), source);
    }
}

/// The fields of a `figure` line after `op`: `fig`, `origin`, `frame`,
/// `target` and `anchor` when there are, `data` and, last, `script`.
pub(super) fn write(o: Obj, name: &str, source: &Source) -> Obj {
    let o = o
        .str("op", "figure")
        .str("fig", name)
        .raw("origin", &Value::from(source.origin.to_vec()).to_string())
        .raw("frame", &Value::from(source.frame.to_vec()).to_string());
    let o = match source.script.target {
        Some(target) => o.raw("target", &Value::from(target.to_vec()).to_string()),
        None => o,
    };
    let o = match &source.script.anchor {
        Some(anchor) => o.str("anchor", anchor),
        None => o,
    };
    o.raw("data", &source.script.data.to_string()).str("script", &source.script.text)
}

/// A `figure` line's change. `None` when a field is missing or not what
/// it should be: the line is then read as one this reader does not know.
pub(super) fn read(m: &Fields) -> Option<Change> {
    let name = m.get("fig")?.as_str()?.to_owned();
    let script = Script {
        text: m.get("script")?.as_str()?.to_owned(),
        data: m.get("data")?.clone(),
        target: match m.get("target") {
            None | Some(Value::Null) => None,
            Some(target) => Some(ints(target)?),
        },
        anchor: match m.get("anchor") {
            None | Some(Value::Null) => None,
            Some(anchor) => Some(anchor.as_str()?.to_owned()),
        },
    };
    let source = Source { script, origin: ints(m.get("origin")?)?, frame: ints(m.get("frame")?)? };
    Some(Change::Figure { name, source })
}

fn ints<const N: usize>(value: &Value) -> Option<[i64; N]> {
    let list = value.as_array()?;
    let ints: Vec<i64> = list.iter().map(Value::as_i64).collect::<Option<_>>()?;
    ints.try_into().ok()
}
