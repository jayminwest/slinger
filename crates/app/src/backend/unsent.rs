//! The pen's strokes the daemon has not written yet, kept on disk
//! (slinger-e76db0): a stroke drawn while the daemon is down survives the
//! app quitting, and goes to the daemon after the next start. One JSON file
//! holds them all, oldest first, and is replaced whole on each change:
//!
//! ```json
//! {"version": 1, "strokes": [{"canvas": "notes", "page": "p1",
//!   "colour": [0, 0, 255], "width": 2.0, "points": [x0, y0, x1, y1]}]}
//! ```
//!
//! Points are canvas units, as the ink holds them. No stroke left: no file.

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use super::{Problem, env_path};
use crate::config::Rgb;
use crate::layout::Point;
use crate::scene::ink::Stroke;

/// The file the unsent strokes are kept in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unsent {
    path: PathBuf,
}

impl Unsent {
    /// `unsent-ink.json` in `$XDG_DATA_HOME/slinger`, else in
    /// `~/Library/Application Support/slinger`, next to the daemon's record.
    /// `None` when neither `XDG_DATA_HOME` nor `HOME` is set.
    pub fn locate() -> Option<Self> {
        let dir = env_path("XDG_DATA_HOME")
            .or_else(|| Some(env_path("HOME")?.join("Library/Application Support")))?;
        Some(Self::at(dir.join("slinger").join("unsent-ink.json")))
    }

    /// The strokes kept in the file at `path`.
    pub fn at(path: PathBuf) -> Self {
        Self { path }
    }

    /// The strokes kept, oldest first; none without a file. A stroke the
    /// file holds but this app cannot read is left out.
    pub fn load(&self) -> Result<Vec<Stroke>, Problem> {
        let text = match fs::read_to_string(&self.path) {
            Ok(text) => text,
            Err(e) if e.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(self.problem(&e)),
        };
        let file: Value = serde_json::from_str(&text).map_err(|e| self.problem(&e))?;
        let strokes = file.get("strokes").and_then(Value::as_array);
        Ok(strokes.into_iter().flatten().filter_map(read).collect())
    }

    /// Keeps `strokes`, oldest first, in place of what was kept: written
    /// whole beside the file, then moved over it, so a quit midway leaves
    /// the old file or the new one. None removes the file.
    pub fn save<'a>(&self, strokes: impl IntoIterator<Item = &'a Stroke>) -> Result<(), Problem> {
        let strokes: Vec<Value> = strokes.into_iter().map(write).collect();
        if strokes.is_empty() {
            return match fs::remove_file(&self.path) {
                Err(e) if e.kind() != ErrorKind::NotFound => Err(self.problem(&e)),
                _ => Ok(()),
            };
        }
        let text = json!({"version": 1, "strokes": strokes}).to_string();
        let beside = self.path.with_extension("json.new");
        let dir = self.path.parent().unwrap_or(Path::new("."));
        fs::create_dir_all(dir)
            .and_then(|()| fs::write(&beside, text))
            .and_then(|()| fs::rename(&beside, &self.path))
            .map_err(|e| self.problem(&e))
    }

    fn problem(&self, e: &dyn std::fmt::Display) -> Problem {
        let hint = "strokes not yet sent cannot be kept: make the file writable, or remove it";
        Problem::new(format!("{}: {e}", self.path.display()), Some(hint))
    }
}

fn write(stroke: &Stroke) -> Value {
    let Rgb { r, g, b } = stroke.colour;
    let points: Vec<f64> = stroke.points.iter().flat_map(|p| [p.x, p.y]).collect();
    json!({"canvas": stroke.canvas, "page": stroke.page, "colour": [r, g, b],
        "width": stroke.width, "points": points})
}

fn read(o: &Value) -> Option<Stroke> {
    let text = |key: &str| o.get(key)?.as_str().map(str::to_string);
    let byte = |v: &Value| u8::try_from(v.as_u64()?).ok();
    let [r, g, b] = o.get("colour")?.as_array()?.as_slice() else { return None };
    let numbers: Vec<f64> =
        o.get("points")?.as_array()?.iter().map(Value::as_f64).collect::<Option<_>>()?;
    if numbers.is_empty() || !numbers.len().is_multiple_of(2) {
        return None;
    }
    Some(Stroke {
        canvas: text("canvas")?,
        page: text("page")?,
        colour: Rgb { r: byte(r)?, g: byte(g)?, b: byte(b)? },
        width: o.get("width")?.as_f64()?,
        points: numbers.as_chunks::<2>().0.iter().map(|[x, y]| Point { x: *x, y: *y }).collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stroke(canvas: &str, x: f64) -> Stroke {
        Stroke {
            canvas: canvas.into(),
            page: "p1".into(),
            colour: Rgb { r: 0, g: 10, b: 255 },
            width: 2.5,
            points: vec![Point { x, y: 1.25 }, Point { x: x + 1.0, y: -2.0 }],
        }
    }

    #[test]
    fn strokes_kept_come_back_in_order_and_none_left_removes_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let file = Unsent::at(dir.path().join("deep/unsent-ink.json"));
        assert_eq!(file.load(), Ok(vec![]), "no file: none kept");
        let kept = [stroke("notes", 0.5), stroke("other", 3.0)];
        file.save(&kept).unwrap();
        assert_eq!(file.load().unwrap(), kept);
        file.save(&kept[1..]).unwrap();
        assert_eq!(file.load().unwrap(), kept[1..]);
        file.save([]).unwrap();
        assert!(!dir.path().join("deep/unsent-ink.json").exists());
        assert!(!dir.path().join("deep/unsent-ink.json.new").exists());
        file.save([]).unwrap();
    }

    #[test]
    fn a_stroke_that_does_not_read_is_left_out_and_a_bad_file_is_a_problem() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("unsent-ink.json");
        let good = write(&stroke("notes", 1.0));
        let odd = json!({"canvas": "notes", "page": "p1", "colour": [0, 0, 0],
            "width": 2.0, "points": [1.0]});
        fs::write(&path, json!({"version": 1, "strokes": [odd, good, 7]}).to_string()).unwrap();
        assert_eq!(Unsent::at(path.clone()).load().unwrap(), [stroke("notes", 1.0)]);
        fs::write(&path, "{nope").unwrap();
        let problem = Unsent::at(path.clone()).load().unwrap_err();
        assert!(problem.message.starts_with(&path.display().to_string()), "{problem:?}");
        assert!(problem.hint.is_some());
    }

    #[test]
    fn the_file_is_beside_the_daemon_s_record() {
        let located = Unsent::locate().map(|f| f.path);
        assert!(located.is_none_or(|p| p.ends_with("slinger/unsent-ink.json")));
    }
}
