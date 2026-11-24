//! SPIKE (slinger-dd9b7b): a canvas text edited by the user's Nvim. The app
//! is an Nvim UI client (`:help ui`): `nvim --embed` runs as a child while
//! the text is edited, draws into a cell grid ([`grid`]) the scene shows in
//! the text's box, and takes the keys ([`keys`]). This module is the pure
//! half: the messages to write ([`Session`]'s methods fill a byte buffer)
//! and what the ones read mean ([`Session::hear`]). The process and its
//! pipes are `backend::nvim`'s.
//!
//! The buffer is a scratch one (`buftype=nofile`, `filetype=markdown`)
//! seeded with the text. Its lines are mirrored here from Nvim's line
//! events (`nvim_buf_attach`), so the text can end from anywhere with what
//! the mirror holds. After each flush the session asks how many screen rows
//! the text takes and how wide the gutter is ([`Session::measure`]) and
//! resizes the UI so the text column stays [`COLUMNS`] cells and the grid
//! is as tall as the text, to [`MOST_ROWS`].

pub mod grid;
pub mod keys;
pub mod pack;

use std::collections::HashMap;

use grid::Grid;
use pack::{Value, encode};

/// How many cells the text column holds: the gutter comes on top.
pub const COLUMNS: usize = 80;
/// The most rows of text the grid grows to; past it Nvim scrolls.
pub const MOST_ROWS: usize = 80;
/// The rows under the text: the command line.
const UNDER: usize = 1;

/// Runs once Nvim has loaded its config: makes the buffer a scratch
/// markdown one holding the lines given, with no undo step for the seed.
const INIT: &str = r"
local lines = ...
local function setup()
local buf = vim.api.nvim_get_current_buf()
vim.g.slinger_win = vim.api.nvim_get_current_win()
vim.bo[buf].buftype = 'nofile'
vim.bo[buf].swapfile = false
local levels = vim.bo[buf].undolevels
vim.bo[buf].undolevels = -1
vim.api.nvim_buf_set_lines(buf, 0, -1, false, lines)
vim.bo[buf].undolevels = levels
vim.bo[buf].filetype = 'markdown'
vim.o.laststatus = 0
vim.o.showtabline = 0
vim.o.cmdheight = 1
vim.wo.wrap = true
-- The owner's pick: a canvas text has no gutter, so the grid is 80 text cells.
vim.wo.number = false
vim.wo.relativenumber = false
vim.wo.signcolumn = 'no'
vim.api.nvim_win_set_cursor(0, { #lines, math.max(#lines[#lines] - 1, 0) })
vim.rpcnotify(0, 'slinger_ready')
end
-- A request that comes right after the attach runs before the user's
-- config: the setup waits for it, so the config cannot undo it.
if vim.v.vim_did_enter == 1 then
  setup()
else
  vim.api.nvim_create_autocmd('VimEnter', { once = true, callback = function() vim.schedule(setup) end })
end
";

/// The screen rows the text takes and the gutter's width.
const MEASURE: &str = r"
local win = vim.g.slinger_win
if not win or not vim.api.nvim_win_is_valid(win) then return { 0, 0 } end
local rows = vim.api.nvim_win_text_height(win, {}).all
-- A grid that grew after the cursor scrolled the window shows the top again.
if rows <= vim.api.nvim_win_get_height(win) and vim.fn.line('w0', win) > 1 then
  vim.api.nvim_win_call(win, function() vim.fn.winrestview({ topline = 1 }) end)
end
return { rows, vim.fn.getwininfo(win)[1].textoff }
";

/// What a request asked, to read its reply by.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Asked {
    Init,
    Measure,
    Barrier,
    Other,
}

/// What a message from Nvim meant.
#[derive(Debug, PartialEq)]
pub enum Heard {
    /// Nothing the window shows.
    Nothing,
    /// The grid is whole again: show it.
    Flushed,
    /// The buffer's lines changed.
    Lines,
    /// The config is loaded and the buffer seeded.
    Ready,
    /// The reply to [`Session::barrier`]: every key sent before it is done.
    Barrier,
    /// A request failed, and what Nvim said.
    Failed(String),
}

/// One attached Nvim, as its client knows it.
#[derive(Debug)]
pub struct Session {
    pub grid: Grid,
    next: i64,
    asked: HashMap<i64, Asked>,
    lines: Vec<String>,
    /// The columns and rows the UI last asked for.
    size: (usize, usize),
    measuring: bool,
    stale: bool,
    /// The gutter's width as last measured: 0 while the override holds.
    pub gutter: usize,
}

impl Session {
    /// A session, number `id`, editing `text`: `out` gets the attach, the
    /// seed and the buffer attach.
    pub fn start(id: u64, text: &str, out: &mut Vec<u8>) -> Self {
        let lines: Vec<String> = text.split('\n').map(str::to_string).collect();
        let size = (COLUMNS, lines.len().clamp(1, MOST_ROWS) + UNDER);
        let mut session = Session {
            grid: Grid::new(id),
            next: 1,
            asked: HashMap::new(),
            lines: lines.clone(),
            size,
            measuring: false,
            stale: false,
            gutter: 0,
        };
        let options = Value::Map(vec![
            (Value::str("ext_linegrid"), Value::Bool(true)),
            (Value::str("rgb"), Value::Bool(true)),
        ]);
        let (cols, rows) = (int(size.0), int(size.1));
        session.request(Asked::Other, "nvim_ui_attach", vec![cols, rows, options], out);
        let seed = Value::Array(lines.iter().map(|l| Value::str(l)).collect());
        let args = vec![Value::str(INIT), Value::Array(vec![seed])];
        session.request(Asked::Init, "nvim_exec_lua", args, out);
        session
    }

    /// The buffer's text as last heard.
    pub fn text(&self) -> String {
        self.lines.join("\n")
    }

    /// Whether Nvim is in normal mode with nothing pending that the mode
    /// says (an operator waiting for its motion is `operator`).
    pub fn normal(&self) -> bool {
        self.grid.mode_name == "normal"
    }

    /// Types `keys` (Nvim's notation).
    pub fn input(&mut self, keys: &str, out: &mut Vec<u8>) {
        self.request(Asked::Other, "nvim_input", vec![Value::str(keys)], out);
    }

    /// A mouse `action` (`press`, `drag`, `release`) of the left button at a cell.
    pub fn mouse(
        &mut self,
        action: &str,
        shift: bool,
        (row, col): (usize, usize),
        out: &mut Vec<u8>,
    ) {
        let modifier = Value::str(if shift { "S" } else { "" });
        let args = vec![
            Value::str("left"),
            Value::str(action),
            modifier,
            Value::Int(0),
            int(row),
            int(col),
        ];
        self.request(Asked::Other, "nvim_input_mouse", args, out);
    }

    /// Pastes `text` at the cursor, in any mode, as one undo step.
    pub fn paste(&mut self, text: &str, out: &mut Vec<u8>) {
        let args = vec![Value::str(text), Value::Bool(true), Value::Int(-1)];
        self.request(Asked::Other, "nvim_paste", args, out);
    }

    /// Runs the Ex command `command`.
    pub fn command(&mut self, command: &str, out: &mut Vec<u8>) {
        self.request(Asked::Other, "nvim_command", vec![Value::str(command)], out);
    }

    /// Asks for a reply that comes once Nvim has taken every key sent so far.
    pub fn barrier(&mut self, out: &mut Vec<u8>) {
        self.request(Asked::Barrier, "nvim_eval", vec![Value::str("1")], out);
    }

    /// Asks how many screen rows the text takes and how wide the gutter
    /// is; one such question is out at a time.
    pub fn measure(&mut self, out: &mut Vec<u8>) {
        if self.measuring {
            self.stale = true;
            return;
        }
        self.measuring = true;
        let args = vec![Value::str(MEASURE), Value::Array(vec![])];
        self.request(Asked::Measure, "nvim_exec_lua", args, out);
    }

    fn request(&mut self, asked: Asked, method: &str, args: Vec<Value>, out: &mut Vec<u8>) {
        let id = self.next;
        self.next += 1;
        if asked != Asked::Other {
            self.asked.insert(id, asked);
        }
        let message = Value::Array(vec![
            Value::Int(0),
            Value::Int(id),
            Value::str(method),
            Value::Array(args),
        ]);
        encode(&message, out);
    }

    /// Takes one message from Nvim; what it asks back goes to `out`.
    pub fn hear(&mut self, message: &Value, out: &mut Vec<u8>) -> Heard {
        let Some([kind, rest @ ..]) = message.as_array() else { return Heard::Nothing };
        match (kind.as_int(), rest) {
            (Some(2), [method, params]) => {
                let params = params.as_array().unwrap_or(&[]);
                match method.as_str() {
                    Some("redraw") if self.grid.redraw(params) => Heard::Flushed,
                    Some("nvim_buf_lines_event") if self.lines_event(params) => Heard::Lines,
                    Some("slinger_ready") => {
                        // Only now: an attach before the config loaded hears nothing.
                        let args = vec![Value::Int(0), Value::Bool(true), Value::Map(vec![])];
                        self.request(Asked::Other, "nvim_buf_attach", args, out);
                        Heard::Ready
                    }
                    _ => Heard::Nothing,
                }
            }
            (Some(1), [id, error, result]) => {
                let asked = id.as_int().and_then(|id| self.asked.remove(&id));
                if *error != Value::Nil {
                    if asked == Some(Asked::Measure) {
                        self.measuring = false;
                    }
                    return Heard::Failed(format!("{error:?}"));
                }
                match asked {
                    Some(Asked::Barrier) => Heard::Barrier,
                    Some(Asked::Measure) => {
                        self.measured(result, out);
                        Heard::Nothing
                    }
                    _ => Heard::Nothing,
                }
            }
            // Nvim asks nothing of this client: a request gets an error, so it never waits.
            (Some(0), [id, ..]) => {
                let reply = Value::Array(vec![
                    Value::Int(1),
                    id.clone(),
                    Value::str("not served"),
                    Value::Nil,
                ]);
                encode(&reply, out);
                Heard::Nothing
            }
            _ => Heard::Nothing,
        }
    }

    /// `nvim_buf_lines_event`: `[buf, tick, first, last, lines, more]`. A
    /// nil tick is a preview ('inccommand'), not a change.
    fn lines_event(&mut self, params: &[Value]) -> bool {
        let [_, tick, first, last, lines, ..] = params else { return false };
        if *tick == Value::Nil {
            return false;
        }
        let new =
            lines.as_array().unwrap_or(&[]).iter().map(|l| l.as_str().unwrap_or("").to_string());
        match (first.as_usize(), last.as_int()) {
            (_, Some(-1)) => self.lines = new.collect(),
            (Some(first), Some(last)) => {
                let last =
                    usize::try_from(last).unwrap_or(0).clamp(first, self.lines.len().max(first));
                let first = first.min(self.lines.len());
                self.lines.splice(first..last.min(self.lines.len()), new);
            }
            _ => return false,
        }
        true
    }

    /// The reply to [`Session::measure`]: resizes the UI when the text's
    /// rows or the gutter changed.
    fn measured(&mut self, result: &Value, out: &mut Vec<u8>) {
        self.measuring = false;
        if let Some([rows, gutter]) = result.as_array()
            && let (Some(rows), Some(gutter)) = (rows.as_usize(), gutter.as_usize())
            && rows > 0
        {
            self.gutter = gutter;
            let want = (COLUMNS + gutter, rows.clamp(1, MOST_ROWS) + UNDER);
            if want != self.size {
                self.size = want;
                self.request(
                    Asked::Other,
                    "nvim_ui_try_resize",
                    vec![int(want.0), int(want.1)],
                    out,
                );
            }
        }
        if std::mem::take(&mut self.stale) {
            self.measure(out);
        }
    }
}

fn int(n: usize) -> Value {
    Value::Int(i64::try_from(n).unwrap_or(i64::MAX))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_events_keep_the_mirror_and_a_preview_does_not() {
        let mut out = Vec::new();
        let mut session = Session::start(1, "one\ntwo\nthree", &mut out);
        let event = |tick: Value, first: i64, last: i64, lines: &[&str]| {
            let lines = Value::Array(lines.iter().map(|l| Value::str(l)).collect());
            let params = vec![
                Value::Int(1),
                tick,
                Value::Int(first),
                Value::Int(last),
                lines,
                Value::Bool(false),
            ];
            Value::Array(vec![
                Value::Int(2),
                Value::str("nvim_buf_lines_event"),
                Value::Array(params),
            ])
        };
        assert_eq!(
            session.hear(&event(Value::Int(3), 1, 2, &["2", "2.5"]), &mut out),
            Heard::Lines
        );
        assert_eq!(session.text(), "one\n2\n2.5\nthree");
        assert_eq!(session.hear(&event(Value::Nil, 0, 1, &["x"]), &mut out), Heard::Nothing);
        assert_eq!(session.hear(&event(Value::Int(4), 0, 4, &[]), &mut out), Heard::Lines);
        assert_eq!(session.text(), "");
    }
}
