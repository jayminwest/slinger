//! A canvas text edited by the user's Nvim (docs/design/writing-column.md,
//! 1A and 6A). The app is an Nvim UI client (`:help ui`): `nvim --embed`
//! runs as a child while the text is edited. This module is the pure half:
//! the messages to write ([`Session`]'s methods fill a byte buffer) and
//! what the ones read mean ([`Session::hear`]). The process and its pipes
//! are `backend::nvim`'s; what the window does with them is
//! `canvases::editing::vim`'s.
//!
//! The order is what keeps the text (slinger-c6e898). The UI attach and the
//! seed go first; the seed runs once the user's config has loaded
//! (`VimEnter`), so the config cannot undo it, and then says
//! `slinger_ready`. Only then does the session attach to the buffer: an
//! attach sent earlier hears nothing. Whatever is asked before ready (keys,
//! pastes, a barrier) is held and written after the attach, in order. The
//! buffer's lines are mirrored from Nvim's line events; the text is
//! **confirmed** ([`Session::text`]) only once the first event has given
//! the whole buffer, so a text that ends before then keeps what it had.
//!
//! Nvim draws into a cell grid ([`grid`]) the scene shows in the text's
//! box (slinger-be5d24). After each flush the session asks how many screen
//! rows the text takes and how wide a gutter a plugin drew anyway
//! ([`Session::measure`]), and resizes the UI so the text keeps
//! [`COLUMNS`] cells and the grid is as tall as the text, to [`MOST_ROWS`],
//! with the command line under it.

pub mod grid;
pub mod keys;
pub mod pack;

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use grid::Grid;
use pack::{Value, encode};

/// Counts the sessions, to tell one's grids from another's.
static SESSIONS: AtomicU64 = AtomicU64::new(1);

/// How many cells the text column holds.
pub const COLUMNS: usize = 80;
/// The most rows of text the grid grows to; past it Nvim scrolls.
pub const MOST_ROWS: usize = 80;
/// The rows under the text: the command line.
const UNDER: usize = 1;

/// Runs once Nvim has loaded the user's config: makes the buffer a scratch
/// markdown one (no file, no swap, no undo file) holding the lines given,
/// with no undo step for the seed, the cursor on the line and byte given
/// (where the text was pressed, or its end), in insert mode for a new text; takes the gutter, the ruler and the status lines away and
/// wraps at word ends, over whatever the config set; then says it is
/// ready. An Nvim older than 0.10 says so instead.
const INIT: &str = r"
local lines, insert, row, col = ...
if vim.fn.has('nvim-0.10') == 0 then
  local v = vim.version()
  vim.rpcnotify(0, 'slinger_old', v.major .. '.' .. v.minor .. '.' .. v.patch)
  return
end
local function setup()
  local buf = vim.api.nvim_get_current_buf()
  vim.bo[buf].buftype = 'nofile'
  vim.bo[buf].swapfile = false
  vim.bo[buf].undofile = false
  local levels = vim.bo[buf].undolevels
  vim.bo[buf].undolevels = -1
  vim.api.nvim_buf_set_lines(buf, 0, -1, false, lines)
  vim.bo[buf].undolevels = levels
  vim.bo[buf].modified = false
  vim.bo[buf].filetype = 'markdown'
  local win = vim.api.nvim_get_current_win()
  vim.g.slinger_win = win
  -- The owner's pick: a canvas text has no gutter, so the grid is 80 text cells.
  for name, value in pairs({ number = false, relativenumber = false, signcolumn = 'no',
      foldcolumn = '0', statuscolumn = '', wrap = true, linebreak = true }) do
    vim.wo[win][name] = value
  end
  vim.o.laststatus, vim.o.showtabline, vim.o.ruler, vim.o.cmdheight = 0, 0, false, 1
  vim.o.breakat = ' '
  vim.api.nvim_win_set_cursor(0, { row, math.max(math.min(col, #lines[row] - 1), 0) })
  if insert then vim.cmd('startinsert!') end
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

/// The screen rows the text takes and the gutter's width. A grid that grew
/// after the cursor scrolled the window (a paste) shows the text's top again.
const MEASURE: &str = r"
local win = vim.g.slinger_win
if not win or not vim.api.nvim_win_is_valid(win) then return { 0, 0 } end
local rows = vim.api.nvim_win_text_height(win, {}).all
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
}

/// What a message from Nvim meant.
#[derive(Debug, PartialEq)]
pub enum Heard {
    /// Nothing the window shows.
    Nothing,
    /// A screen is whole: Nvim flushed its redraw.
    Flushed,
    /// The buffer's lines changed, and the text is confirmed.
    Lines,
    /// The config is loaded and the buffer seeded: what was held is sent.
    Ready,
    /// The word [`Session::barrier`] asked for: every key sent before it is done.
    Barrier,
    /// This Nvim is older than 0.10 (its version): it cannot edit a text.
    Old(String),
    /// The seed failed, and what Nvim said: the session never gets ready.
    Broken(String),
    /// Another request failed, and what Nvim said.
    Failed(String),
}

/// One attached Nvim, as its client knows it.
#[derive(Debug)]
pub struct Session {
    next: i64,
    asked: HashMap<i64, Asked>,
    /// The buffer's lines as Nvim last gave them: none until the first line
    /// event gave them whole.
    lines: Option<Vec<String>>,
    ready: bool,
    /// The requests asked before ready, written after it, in order.
    held: Vec<u8>,
    /// Nvim's screen, its mode among it.
    grid: Grid,
    /// Whether a measure is out, and whether another flush came since.
    measuring: bool,
    stale: bool,
    /// The barriers asked, and the mode (`mode(1)`) the last one found.
    barriers: i64,
    settled: Option<String>,
}

impl Session {
    /// A session editing `text` (`insert`: in insert mode, as a new text
    /// starts), its cursor at the byte `caret`: `out` gets the UI attach
    /// and the seed.
    pub fn start(text: &str, (insert, caret): (bool, usize), out: &mut Vec<u8>) -> Self {
        let lines: Vec<&str> = text.split('\n').collect();
        let size = (COLUMNS, lines.len().clamp(1, MOST_ROWS) + UNDER);
        let mut session = Session {
            next: 1,
            asked: HashMap::new(),
            lines: None,
            ready: false,
            held: Vec::new(),
            grid: Grid::new(SESSIONS.fetch_add(1, Ordering::Relaxed)),
            measuring: false,
            stale: false,
            barriers: 0,
            settled: None,
        };
        let options = Value::Map(vec![
            (Value::str("ext_linegrid"), Value::Bool(true)),
            (Value::str("rgb"), Value::Bool(true)),
        ]);
        session.write(None, "nvim_ui_attach", vec![int(size.0), int(size.1), options], out);
        let seed = Value::Array(lines.iter().map(|l| Value::str(l)).collect());
        let before = &text[..text.floor_char_boundary(caret.min(text.len()))];
        let row = before.matches('\n').count() + 1;
        let col = before.len() - before.rfind('\n').map_or(0, |n| n + 1);
        let cursor = [Value::Bool(insert), int(row), int(col)];
        let args = vec![Value::str(INIT), Value::Array([vec![seed], cursor.to_vec()].concat())];
        session.write(Some(Asked::Init), "nvim_exec_lua", args, out);
        session
    }

    /// The buffer's text, once confirmed: `None` until Nvim has given the
    /// whole buffer, so a text that ends before then keeps the one it had.
    pub fn text(&self) -> Option<String> {
        self.lines.as_ref().map(|lines| lines.join("\n"))
    }

    /// Whether the config has loaded and the buffer is seeded.
    pub fn ready(&self) -> bool {
        self.ready
    }

    /// Whether Nvim was in normal mode, with nothing pending, when the
    /// last barrier was taken: false until it is, and in any other mode
    /// (an operator waiting for its motion, a normal command from insert).
    pub fn normal(&self) -> bool {
        self.settled.as_deref() == Some("n")
    }

    /// Nvim's screen as last heard: whole after [`Heard::Flushed`].
    pub fn grid(&self) -> &Grid {
        &self.grid
    }

    /// Whether Nvim is in visual mode (by character, line or block): a
    /// copy or a cut takes what it selects.
    pub fn visual(&self) -> bool {
        self.grid.mode_name == "visual"
    }

    /// Types `keys` (Nvim's notation).
    pub fn input(&mut self, keys: &str, out: &mut Vec<u8>) {
        self.request(None, "nvim_input", vec![Value::str(keys)], out);
    }

    /// The left button's `action` (`press`, `drag` or `release`) at the
    /// grid's cell `(row, column)`, with `shift` held or not. It goes in
    /// with the keys, in turn.
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
        self.request(None, "nvim_input_mouse", args, out);
    }

    /// Pastes `text` at the cursor, in any mode, as one undo step.
    pub fn paste(&mut self, text: &str, out: &mut Vec<u8>) {
        let args = vec![Value::str(text), Value::Bool(true), Value::Int(-1)];
        self.request(None, "nvim_paste", args, out);
    }

    /// Asks for word ([`Heard::Barrier`]) once Nvim has taken every key
    /// sent so far, with the mode they left it in ([`Session::normal`]).
    /// It goes in with the keys, in turn: Nvim may answer a request before
    /// it takes keys sent ahead of it (a lone escape is held for
    /// 'ttimeoutlen'), and sends a mode change only when it next flushes.
    /// A pending operator or count is ended by it, as an escape would.
    pub fn barrier(&mut self, out: &mut Vec<u8>) {
        self.barriers += 1;
        self.settled = None;
        let keys =
            format!("<Cmd>call rpcnotify(0, 'slinger_barrier', {}, mode(1))<CR>", self.barriers);
        self.input(&keys, out);
    }

    /// A request: written to `out` once ready, held until then.
    fn request(&mut self, asked: Option<Asked>, method: &str, args: Vec<Value>, out: &mut Vec<u8>) {
        if self.ready {
            self.write(asked, method, args, out);
        } else {
            let mut held = std::mem::take(&mut self.held);
            self.write(asked, method, args, &mut held);
            self.held = held;
        }
    }

    fn write(&mut self, asked: Option<Asked>, method: &str, args: Vec<Value>, out: &mut Vec<u8>) {
        let id = self.next;
        self.next += 1;
        if let Some(asked) = asked {
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
                    Some("redraw") if self.grid.redraw(params) => {
                        if self.ready {
                            self.measure(out);
                        }
                        Heard::Flushed
                    }
                    Some("nvim_buf_lines_event") if self.lines_event(params) => Heard::Lines,
                    Some("slinger_ready") if !self.ready => {
                        // Only now: an attach before the config loaded hears nothing.
                        let args = vec![Value::Int(0), Value::Bool(true), Value::Map(vec![])];
                        self.write(None, "nvim_buf_attach", args, out);
                        out.append(&mut self.held);
                        self.ready = true;
                        Heard::Ready
                    }
                    Some("slinger_barrier") => match params {
                        [Value::Int(n), mode] if *n == self.barriers => {
                            self.settled = mode.as_str().map(str::to_string);
                            Heard::Barrier
                        }
                        _ => Heard::Nothing,
                    },
                    Some("slinger_old") => {
                        Heard::Old(params.first().and_then(Value::as_str).unwrap_or("").into())
                    }
                    _ => Heard::Nothing,
                }
            }
            (Some(1), [id, error, result]) => {
                let asked = id.as_int().and_then(|id| self.asked.remove(&id));
                match asked {
                    _ if *error != Value::Nil && asked == Some(Asked::Init) => {
                        Heard::Broken(said(error))
                    }
                    _ if *error != Value::Nil => {
                        self.measuring &= asked != Some(Asked::Measure);
                        Heard::Failed(said(error))
                    }
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

    /// Asks how many screen rows the text takes and how wide the gutter
    /// is; one such question is out at a time, and a flush while it is out
    /// asks again once it is answered.
    fn measure(&mut self, out: &mut Vec<u8>) {
        if self.measuring {
            self.stale = true;
            return;
        }
        self.measuring = true;
        let args = vec![Value::str(MEASURE), Value::Array(vec![])];
        self.write(Some(Asked::Measure), "nvim_exec_lua", args, out);
    }

    /// The answer to [`Session::measure`]: `[rows, gutter]`. Resizes the
    /// UI when the grid it asks for changed.
    fn measured(&mut self, result: &Value, out: &mut Vec<u8>) {
        self.measuring = false;
        if let Some([rows, gutter]) = result.as_array()
            && let (Some(rows @ 1..), Some(gutter)) = (rows.as_usize(), gutter.as_usize())
        {
            // Against the grid Nvim drew, not the size asked for: Nvim makes
            // the first grid tall enough for its status line, gone since.
            let want = (COLUMNS + gutter, rows.min(MOST_ROWS) + UNDER);
            if want != (self.grid.cols, self.grid.rows) {
                self.write(None, "nvim_ui_try_resize", vec![int(want.0), int(want.1)], out);
            }
        }
        if std::mem::take(&mut self.stale) {
            self.measure(out);
        }
    }

    /// `nvim_buf_lines_event`: `[buf, tick, first, last, lines, more]`. A
    /// nil tick is a preview ('inccommand'), not a change; before the first
    /// whole buffer, a part of one means nothing.
    fn lines_event(&mut self, params: &[Value]) -> bool {
        let [_, tick, first, last, lines, ..] = params else { return false };
        if *tick == Value::Nil {
            return false;
        }
        let new =
            lines.as_array().unwrap_or(&[]).iter().map(|l| l.as_str().unwrap_or("").to_string());
        match (&mut self.lines, first.as_usize(), last.as_int()) {
            (held, Some(0), Some(-1)) => *held = Some(new.collect()),
            (Some(held), Some(first), Some(last)) => {
                let last = usize::try_from(last).unwrap_or(0).clamp(first, held.len().max(first));
                let first = first.min(held.len());
                held.splice(first..last.min(held.len()), new);
            }
            _ => return false,
        }
        true
    }
}

/// What an error from Nvim says: `[kind, message]`.
fn said(error: &Value) -> String {
    match error.as_array() {
        Some([_, message]) => message.as_str().unwrap_or("").to_string(),
        _ => format!("{error:?}"),
    }
}

fn int(n: usize) -> Value {
    Value::Int(i64::try_from(n).unwrap_or(i64::MAX))
}

#[cfg(test)]
mod tests;
