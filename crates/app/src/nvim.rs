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

pub mod keys;
pub mod pack;

use std::collections::HashMap;

use pack::{Value, encode};

/// How many cells the text column holds.
pub const COLUMNS: usize = 80;
/// The most rows of text the grid grows to; past it Nvim scrolls.
pub const MOST_ROWS: usize = 80;
/// The rows under the text: the command line.
const UNDER: usize = 1;

/// Runs once Nvim has loaded the user's config: makes the buffer a scratch
/// markdown one (no file, no swap, no undo file) holding the lines given,
/// with no undo step for the seed, the cursor at the end, in insert mode for
/// a new text; then says it is ready. An Nvim older than 0.10 says so instead.
const INIT: &str = r"
local lines, insert = ...
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
  vim.api.nvim_win_set_cursor(0, { #lines, math.max(#lines[#lines] - 1, 0) })
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

/// What a request asked, to read its reply by.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Asked {
    Init,
    Barrier,
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
    /// The reply to [`Session::barrier`]: every key sent before it is done.
    Barrier,
    /// This Nvim is older than 0.10 (its version): it cannot edit a text.
    Old(String),
    /// The seed failed, and what Nvim said: the session never gets ready.
    Broken(String),
    /// Another request failed, and what Nvim said.
    Failed(String),
}

/// One attached Nvim, as its client knows it.
#[derive(Debug, Default)]
pub struct Session {
    next: i64,
    asked: HashMap<i64, Asked>,
    /// The buffer's lines as Nvim last gave them: none until the first line
    /// event gave them whole.
    lines: Option<Vec<String>>,
    ready: bool,
    /// The requests asked before ready, written after it, in order.
    held: Vec<u8>,
    /// Nvim's mode, by its short name (`normal`, `insert`, …).
    mode: String,
}

impl Session {
    /// A session editing `text` (`insert`: in insert mode, as a new text
    /// starts): `out` gets the UI attach and the seed.
    pub fn start(text: &str, insert: bool, out: &mut Vec<u8>) -> Self {
        let lines: Vec<&str> = text.split('\n').collect();
        let mut session = Session { next: 1, ..Session::default() };
        let options = Value::Map(vec![
            (Value::str("ext_linegrid"), Value::Bool(true)),
            (Value::str("rgb"), Value::Bool(true)),
        ]);
        let rows = int(lines.len().clamp(1, MOST_ROWS) + UNDER);
        session.write(None, "nvim_ui_attach", vec![int(COLUMNS), rows, options], out);
        let seed = Value::Array(lines.iter().map(|l| Value::str(l)).collect());
        let args = vec![Value::str(INIT), Value::Array(vec![seed, Value::Bool(insert)])];
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

    /// Whether Nvim is in normal mode with nothing pending that the mode
    /// says (an operator waiting for its motion is `operator`).
    pub fn normal(&self) -> bool {
        self.mode == "normal"
    }

    /// Types `keys` (Nvim's notation).
    pub fn input(&mut self, keys: &str, out: &mut Vec<u8>) {
        self.request(None, "nvim_input", vec![Value::str(keys)], out);
    }

    /// Pastes `text` at the cursor, in any mode, as one undo step.
    pub fn paste(&mut self, text: &str, out: &mut Vec<u8>) {
        let args = vec![Value::str(text), Value::Bool(true), Value::Int(-1)];
        self.request(None, "nvim_paste", args, out);
    }

    /// Asks for a reply that comes once Nvim has taken every key sent so far.
    pub fn barrier(&mut self, out: &mut Vec<u8>) {
        self.request(Some(Asked::Barrier), "nvim_eval", vec![Value::str("1")], out);
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
                    Some("redraw") if self.redraw(params) => Heard::Flushed,
                    Some("nvim_buf_lines_event") if self.lines_event(params) => Heard::Lines,
                    Some("slinger_ready") if !self.ready => {
                        // Only now: an attach before the config loaded hears nothing.
                        let args = vec![Value::Int(0), Value::Bool(true), Value::Map(vec![])];
                        self.write(None, "nvim_buf_attach", args, out);
                        out.append(&mut self.held);
                        self.ready = true;
                        Heard::Ready
                    }
                    Some("slinger_old") => {
                        Heard::Old(params.first().and_then(Value::as_str).unwrap_or("").into())
                    }
                    _ => Heard::Nothing,
                }
            }
            (Some(1), [id, error, _]) => {
                let asked = id.as_int().and_then(|id| self.asked.remove(&id));
                match asked {
                    _ if *error != Value::Nil && asked == Some(Asked::Init) => {
                        Heard::Broken(said(error))
                    }
                    _ if *error != Value::Nil => Heard::Failed(said(error)),
                    Some(Asked::Barrier) => Heard::Barrier,
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

    /// One `redraw` notification: its mode changes, and whether it ends
    /// in a flush.
    fn redraw(&mut self, batches: &[Value]) -> bool {
        let mut flushed = false;
        for batch in batches {
            let Some([name, calls @ ..]) = batch.as_array() else { continue };
            match name.as_str() {
                Some("mode_change") => {
                    let last = calls.last().and_then(Value::as_array);
                    if let Some(mode) = last.and_then(<[Value]>::first).and_then(Value::as_str) {
                        mode.clone_into(&mut self.mode);
                    }
                }
                Some("flush") => flushed = true,
                _ => {}
            }
        }
        flushed
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
