//! Nvim's screen as a UI client holds it (`:help ui-linegrid`): the cells of
//! grid 1, each with its text and a highlight id, the highlight table, the
//! default colours, the cursor and the mode. [`Grid::redraw`] takes one
//! `redraw` batch; `platform` paints what [`Grid::style`] and
//! [`Grid::cursor_shape`] say. Pure. With `ext_linegrid` alone Nvim draws
//! floats, the popup menu, the command line and messages into this one grid.

use std::collections::HashMap;

use super::pack::Value;
use crate::config::Rgb;

/// One screen cell: its text (empty for the right half of a wide
/// character) and its highlight id.
#[derive(Debug, Clone, Default)]
pub struct Cell {
    pub text: String,
    pub hl: u64,
}

/// How a cell is underlined.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Under {
    #[default]
    None,
    Line,
    Curl,
    Double,
    Dotted,
    Dashed,
}

/// A highlight as `hl_attr_define` gave it: a colour it does not name is the default's.
#[derive(Debug, Clone, Copy, Default)]
#[expect(clippy::struct_excessive_bools, reason = "one bool per attribute hl_attr_define names")]
struct Hl {
    fg: Option<Rgb>,
    bg: Option<Rgb>,
    sp: Option<Rgb>,
    reverse: bool,
    bold: bool,
    italic: bool,
    strike: bool,
    under: Under,
}

/// A highlight resolved against the default colours: what to paint.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Style {
    pub fg: Rgb,
    pub bg: Rgb,
    pub sp: Rgb,
    pub bold: bool,
    pub italic: bool,
    pub strike: bool,
    pub under: Under,
}

/// The cursor's shape in a mode.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CursorShape {
    Block,
    /// A bar at the cell's left, this part of its width.
    Vertical(f64),
    /// A bar at the cell's bottom, this part of its height.
    Horizontal(f64),
}

#[derive(Debug, Clone)]
struct Mode {
    shape: CursorShape,
}

/// Grid 1 and what paints it.
#[derive(Debug, Clone)]
pub struct Grid {
    pub cols: usize,
    pub rows: usize,
    cells: Vec<Cell>,
    /// The cursor's row and column.
    pub cursor: (usize, usize),
    hls: HashMap<u64, Hl>,
    fg: Rgb,
    bg: Rgb,
    sp: Rgb,
    modes: Vec<Mode>,
    mode: usize,
    /// The mode's name, as `mode_change` said it: `normal`, `insert`, ...
    pub mode_name: String,
    /// Whether Nvim said to hide the cursor (`busy_start`).
    pub busy: bool,
    /// Counts the flushes: two grids of one session are the same picture
    /// when it is the same.
    pub serial: u64,
    /// Tells one session's grids from another's.
    pub session: u64,
}

/// The same picture: the same flush of the same session. Comparing every
/// cell at each scene would cost more than painting them.
impl PartialEq for Grid {
    fn eq(&self, other: &Self) -> bool {
        self.session == other.session && self.serial == other.serial
    }
}

impl Grid {
    /// An empty grid of session `session`.
    pub fn new(session: u64) -> Self {
        Grid {
            cols: 0,
            rows: 0,
            cells: Vec::new(),
            cursor: (0, 0),
            hls: HashMap::new(),
            fg: Rgb { r: 0xe6, g: 0xe6, b: 0xe6 },
            bg: Rgb { r: 0x1e, g: 0x1e, b: 0x1e },
            sp: Rgb { r: 0xff, g: 0x6b, b: 0x6b },
            modes: Vec::new(),
            mode: 0,
            mode_name: String::new(),
            busy: false,
            serial: 0,
            session,
        }
    }

    /// The cells of `row`.
    pub fn row(&self, row: usize) -> &[Cell] {
        self.cells.get(row * self.cols..(row + 1) * self.cols).unwrap_or(&[])
    }

    /// The text of `row`, its trailing blanks cut.
    pub fn row_text(&self, row: usize) -> String {
        let text: String = self.row(row).iter().map(|c| c.text.as_str()).collect();
        text.trim_end().to_string()
    }

    /// The default background.
    pub fn background(&self) -> Rgb {
        self.bg
    }

    /// What highlight `id` paints.
    pub fn style(&self, id: u64) -> Style {
        let hl = self.hls.get(&id).copied().unwrap_or_default();
        let (mut fg, mut bg) = (hl.fg.unwrap_or(self.fg), hl.bg.unwrap_or(self.bg));
        if hl.reverse {
            std::mem::swap(&mut fg, &mut bg);
        }
        let sp = hl.sp.unwrap_or(fg);
        Style { fg, bg, sp, bold: hl.bold, italic: hl.italic, strike: hl.strike, under: hl.under }
    }

    /// The cursor's shape now; `None` while Nvim hides it.
    pub fn cursor_shape(&self) -> Option<CursorShape> {
        if self.busy {
            return None;
        }
        Some(self.modes.get(self.mode).map_or(CursorShape::Block, |m| m.shape))
    }

    /// Takes the events of one `redraw` notification (its parameters).
    /// Returns whether a `flush` was among them: the grid is whole again.
    pub fn redraw(&mut self, batch: &[Value]) -> bool {
        let mut flushed = false;
        for event in batch {
            let Some([name, calls @ ..]) = event.as_array() else { continue };
            let Some(name) = name.as_str() else { continue };
            if name == "flush" {
                flushed = true;
                self.serial += 1;
                continue;
            }
            if name == "busy_start" || name == "busy_stop" {
                self.busy = name == "busy_start";
                continue;
            }
            for call in calls {
                if let Some(args) = call.as_array() {
                    self.event(name, args);
                }
            }
        }
        flushed
    }

    fn event(&mut self, name: &str, args: &[Value]) {
        let int = |i: usize| args.get(i).and_then(Value::as_usize).unwrap_or(0);
        // Only grid 1 exists without ext_multigrid.
        let one = || args.first().and_then(Value::as_int) == Some(1);
        match name {
            "grid_resize" if one() => self.resize(int(1), int(2)),
            "grid_clear" if one() => self.cells.fill(Cell { text: " ".into(), hl: 0 }),
            "grid_cursor_goto" if one() => self.cursor = (int(1), int(2)),
            "grid_line" if one() => {
                let cells = args.get(3).and_then(Value::as_array).unwrap_or(&[]);
                self.line(int(1), int(2), cells);
            }
            "grid_scroll" if one() => {
                let rows = args.get(5).and_then(Value::as_int).unwrap_or(0);
                self.scroll((int(1), int(2)), (int(3), int(4)), rows);
            }
            "default_colors_set" => {
                let colour = |i: usize| args.get(i).and_then(Value::as_int).and_then(rgb);
                self.fg = colour(0).unwrap_or(self.fg);
                self.bg = colour(1).unwrap_or(self.bg);
                self.sp = colour(2).unwrap_or(self.sp);
            }
            "hl_attr_define" => {
                if let (Some(id), Some(attrs)) = (args.first().and_then(Value::as_int), args.get(1))
                {
                    self.hls.insert(id.cast_unsigned(), hl(attrs));
                }
            }
            "mode_info_set" => {
                let modes = args.get(1).and_then(Value::as_array).unwrap_or(&[]);
                self.modes = modes.iter().map(mode).collect();
            }
            "mode_change" => {
                self.mode_name = args.first().and_then(Value::as_str).unwrap_or("").to_string();
                self.mode = int(1);
            }
            _ => {}
        }
    }

    fn resize(&mut self, cols: usize, rows: usize) {
        let mut cells = vec![Cell { text: " ".into(), hl: 0 }; cols * rows];
        for row in 0..rows.min(self.rows) {
            for col in 0..cols.min(self.cols) {
                cells[row * cols + col] = std::mem::take(&mut self.cells[row * self.cols + col]);
            }
        }
        (self.cols, self.rows, self.cells) = (cols, rows, cells);
    }

    /// `grid_line`: each of `cells` is `[text, hl?, repeat?]` from `col` on.
    fn line(&mut self, row: usize, mut col: usize, cells: &[Value]) {
        let mut hl = 0;
        for cell in cells {
            let Some([text, more @ ..]) = cell.as_array() else { continue };
            let text = text.as_str().unwrap_or(" ");
            if let Some(id) = more.first().and_then(Value::as_int) {
                hl = id.cast_unsigned();
            }
            let repeat = more.get(1).and_then(Value::as_usize).unwrap_or(1);
            for _ in 0..repeat {
                if row < self.rows && col < self.cols {
                    self.cells[row * self.cols + col] = Cell { text: text.to_string(), hl };
                }
                col += 1;
            }
        }
    }

    /// `grid_scroll`: the rows `top..bot` of the columns `left..right` move
    /// up by `rows` (down when negative).
    fn scroll(&mut self, (top, bot): (usize, usize), (left, right): (usize, usize), rows: i64) {
        let (bot, right) = (bot.min(self.rows), right.min(self.cols));
        let by = usize::try_from(rows.unsigned_abs()).unwrap_or(usize::MAX);
        if by == 0 || top + by >= bot {
            return;
        }
        let cols = self.cols;
        let mut copy = |to: usize, from: usize| {
            for col in left..right {
                self.cells[to * cols + col] = self.cells[from * cols + col].clone();
            }
        };
        if rows > 0 {
            (top..bot - by).for_each(|row| copy(row, row + by));
        } else {
            (top + by..bot).rev().for_each(|row| copy(row, row - by));
        }
    }
}

fn rgb(n: i64) -> Option<Rgb> {
    let n = u32::try_from(n).ok()?;
    let [_, r, g, b] = n.to_be_bytes();
    Some(Rgb { r, g, b })
}

fn hl(attrs: &Value) -> Hl {
    let on = |key: &str| attrs.get(key).is_some_and(Value::truthy);
    let colour = |key: &str| attrs.get(key).and_then(Value::as_int).and_then(rgb);
    let unders = [
        ("undercurl", Under::Curl),
        ("underdouble", Under::Double),
        ("underdotted", Under::Dotted),
        ("underdashed", Under::Dashed),
        ("underline", Under::Line),
    ];
    Hl {
        fg: colour("foreground"),
        bg: colour("background"),
        sp: colour("special"),
        reverse: on("reverse"),
        bold: on("bold"),
        italic: on("italic"),
        strike: on("strikethrough"),
        under: unders.iter().find(|(key, _)| on(key)).map_or(Under::None, |(_, under)| *under),
    }
}

fn mode(info: &Value) -> Mode {
    let part = info.get("cell_percentage").and_then(Value::as_int).unwrap_or(100);
    #[expect(clippy::cast_precision_loss, reason = "a percentage")]
    let part = (part as f64 / 100.0).clamp(0.05, 1.0);
    let shape = match info.get("cursor_shape").and_then(Value::as_str) {
        Some("vertical") => CursorShape::Vertical(part),
        Some("horizontal") => CursorShape::Horizontal(part),
        _ => CursorShape::Block,
    };
    Mode { shape }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a(items: Vec<Value>) -> Value {
        Value::Array(items)
    }

    fn cell(text: &str, more: &[i64]) -> Value {
        let mut items = vec![Value::str(text)];
        items.extend(more.iter().map(|n| Value::Int(*n)));
        a(items)
    }

    #[test]
    fn a_batch_resizes_writes_lines_scrolls_and_flushes() {
        let mut grid = Grid::new(1);
        let n = Value::Int;
        let line = |row, col, cells| a(vec![n(1), n(row), n(col), a(cells), Value::Bool(false)]);
        let batch = [
            a(vec![Value::str("grid_resize"), a(vec![n(1), n(4), n(3)])]),
            a(vec![
                Value::str("grid_line"),
                line(0, 0, vec![cell("a", &[7]), cell("b", &[]), cell(" ", &[0, 2])]),
                line(1, 1, vec![cell("漢", &[0]), cell("", &[])]),
            ]),
            a(vec![Value::str("grid_cursor_goto"), a(vec![n(1), n(1), n(1)])]),
            a(vec![Value::str("flush"), a(vec![])]),
        ];
        assert!(grid.redraw(&batch));
        assert_eq!((grid.row_text(0), grid.row_text(1)), ("ab".to_string(), " 漢".to_string()));
        assert_eq!((grid.row(0)[1].hl, grid.cursor, grid.serial), (7, (1, 1), 1));
        let up = a(vec![n(1), n(0), n(3), n(0), n(4), n(1), n(0)]);
        assert!(!grid.redraw(&[a(vec![Value::str("grid_scroll"), up])]));
        assert_eq!(grid.row_text(0), " 漢");
    }

    #[test]
    fn a_highlight_takes_the_default_colours_it_does_not_name() {
        let mut grid = Grid::new(1);
        let attrs = Value::Map(vec![
            (Value::str("foreground"), Value::Int(0x00ff_0000)),
            (Value::str("reverse"), Value::Bool(true)),
            (Value::str("undercurl"), Value::Bool(true)),
        ]);
        let define = a(vec![Value::Int(3), attrs, Value::Map(vec![]), a(vec![])]);
        grid.redraw(&[a(vec![Value::str("hl_attr_define"), define])]);
        let style = grid.style(3);
        assert_eq!(style.bg, Rgb { r: 255, g: 0, b: 0 });
        assert_eq!((style.fg, style.under), (grid.background(), Under::Curl));
    }
}
