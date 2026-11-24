//! SPIKE (slinger-dd9b7b): the Nvim that edits the text being typed
//! ([`crate::nvim`], `backend::nvim`). [`Editing`] starts one when a text
//! opens ([`Editing::vim_sync`]), hands it the keys, the pastes and the
//! presses inside its grid, and takes its news ([`Editing::vim_poll`]):
//! each flushed grid goes to the scene, each change of the buffer's lines
//! to the text, so the text ends, from anywhere, with what Nvim holds.
//! Escape in normal mode ends the text; so does Nvim ending (`:q`, `ZZ`).
//! With no `SLINGER_NVIM`, or no `nvim` to start, the scene's own editor
//! types the text as before.

use std::collections::VecDeque;
use std::fs::File;
use std::io::Write;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use super::Editing;
use crate::backend::Wake;
use crate::backend::nvim::{Process, Setup};
use crate::canvases::follow::Follow;
use crate::commands::{CMD, Chord};
use crate::layout::Point;
use crate::nvim::pack::Value;
use crate::nvim::{Heard, Session, keys};
use crate::scene::shapes::{REACH, Shapes};

/// Counts the sessions, to tell their grids apart.
static SESSIONS: AtomicU64 = AtomicU64::new(1);
/// The most a leave waits for Nvim to take the keys already sent.
const SETTLE: Duration = Duration::from_millis(250);

/// One text's Nvim.
pub(super) struct Vim {
    process: Process,
    session: Session,
    /// When each key not yet shown was sent.
    keys: VecDeque<Instant>,
    /// For each key shown but not painted: when it was sent, when the
    /// flush that showed it came off the pipe, and when the main thread took it.
    unpainted: Vec<(Instant, Instant, Instant)>,
    trace: Option<File>,
    started: Instant,
    flushed: bool,
    /// Whether the left button is down inside the grid.
    pressed: bool,
}

impl Vim {
    fn start(setup: &Setup, wake: &Wake, text: &str) -> std::io::Result<Vim> {
        let started = Instant::now();
        let mut process = Process::start(setup, wake)?;
        let mut out = Vec::new();
        let session = Session::start(SESSIONS.fetch_add(1, Ordering::Relaxed), text, &mut out);
        process.send(&out);
        let trace = setup
            .trace
            .as_ref()
            .and_then(|path| std::fs::OpenOptions::new().create(true).append(true).open(path).ok());
        let (keys, unpainted) = (VecDeque::new(), Vec::new());
        let mut vim = Vim {
            process,
            session,
            keys,
            unpainted,
            trace,
            started,
            flushed: false,
            pressed: false,
        };
        vim.say(&format!("spawned_us {}", started.elapsed().as_micros()));
        Ok(vim)
    }

    /// A line of the trace, if one is kept.
    fn say(&mut self, line: &str) {
        if let Some(trace) = &mut self.trace {
            // A trace that cannot be written is not worth stopping for.
            let _ = writeln!(trace, "{line}");
        }
    }

    /// Takes one message, heard at `at`: a flushed grid and changed lines
    /// go to `shapes`. Returns what it was.
    fn take(&mut self, message: &Value, at: Instant, shapes: &mut Shapes) -> Heard {
        let mut out = Vec::new();
        let heard = self.session.hear(message, &mut out);
        match &heard {
            Heard::Flushed => {
                shapes.set_grid(Arc::new(self.session.grid.clone()));
                if !std::mem::replace(&mut self.flushed, true) {
                    self.say(&format!("first_flush_us {}", (at - self.started).as_micros()));
                }
                if let Some(key) = self.keys.pop_front() {
                    self.unpainted.push((key, at, Instant::now()));
                }
                self.session.measure(&mut out);
            }
            Heard::Lines => shapes.set_text(self.session.text()),
            Heard::Ready => self.say(&format!("ready_us {}", (at - self.started).as_micros())),
            Heard::Failed(why) => self.say(&format!("failed {why}")),
            Heard::Nothing | Heard::Barrier => {}
        }
        if !out.is_empty() {
            self.process.send(&out);
        }
        heard
    }

    /// Waits, at most [`SETTLE`], until Nvim has taken every key sent so
    /// far, so its mode and the text are current.
    fn settle(&mut self, shapes: &mut Shapes) {
        let mut out = Vec::new();
        self.session.barrier(&mut out);
        self.process.send(&out);
        let until = Instant::now() + SETTLE;
        while let Some((message, at)) =
            self.process.wait(until.saturating_duration_since(Instant::now()))
        {
            if self.take(&message, at, shapes) == Heard::Barrier {
                break;
            }
        }
    }

    fn input(&mut self, keys: &str) {
        if keys.is_empty() {
            return;
        }
        let mut out = Vec::new();
        self.session.input(keys, &mut out);
        self.process.send(&out);
        // A queue this long is of keys no flush answered one for one.
        if self.keys.len() > 8 {
            self.keys.clear();
        }
        self.keys.push_back(Instant::now());
    }
}

impl Editing {
    /// Starts an Nvim for a text that opened, and lets go of the one whose
    /// text ended. With no Nvim to start, the scene's editor types on.
    pub(super) fn vim_sync(&mut self) {
        if self.vim.is_some() && !self.shapes.vimming() {
            self.vim = None;
        }
        if self.vim.is_some() || !self.shapes.editing() {
            return;
        }
        let Some(setup) = &self.vim_setup else { return };
        let text = self.shapes.typing_text().unwrap_or_default();
        match Vim::start(setup, &self.wake, &text) {
            Ok(vim) => {
                self.shapes.vim_opened();
                self.vim = Some(vim);
            }
            // No Nvim: the scene's editor from now on.
            Err(_) => self.vim_setup = None,
        }
    }

    /// Takes Nvim's news. An Nvim that ended (`:q`, `ZZ`) ends its text.
    /// Returns whether the window shows something new.
    pub(in crate::canvases) fn vim_poll(&mut self, follow: &mut Follow) -> bool {
        self.vim_sync();
        let Some(vim) = &mut self.vim else { return false };
        let (messages, gone) = vim.process.heard();
        let mut changed = false;
        for (message, at) in messages {
            changed |= vim.take(&message, at, &mut self.shapes) == Heard::Flushed;
        }
        if gone {
            vim.say("gone");
            self.shapes.end(&mut self.ink);
            self.vim = None;
            self.send(follow);
            return true;
        }
        changed
    }

    /// A key press, as its chord: `None` when no Nvim edits a text. Escape
    /// in normal mode ends the text; a named key or a control chord goes
    /// to Nvim; cmd+A selects the buffer. Any other chord is not Nvim's
    /// (false): a key that types comes again as its text.
    pub(super) fn vim_key(&mut self, follow: &mut Follow, chord: Chord) -> Option<bool> {
        let vim = self.vim.as_mut()?;
        if chord == Chord::new(0, '\u{1b}') {
            vim.settle(&mut self.shapes);
            if vim.session.normal() {
                vim.say("leave escape");
                self.shapes.end(&mut self.ink);
                self.vim = None;
                self.send(follow);
                return Some(true);
            }
        }
        if chord == Chord::new(CMD, 'a') {
            vim.input("<Esc>ggVG");
            return Some(true);
        }
        let Some(keys) = keys::chord(chord) else { return Some(false) };
        vim.input(&keys);
        Some(true)
    }

    /// Typed `text` goes to Nvim. False when no Nvim edits a text.
    pub(super) fn vim_typed(&mut self, text: &str) -> bool {
        let Some(vim) = &mut self.vim else { return false };
        vim.input(&keys::text(text));
        true
    }

    /// Pasted `text` goes in at Nvim's cursor (`nvim_paste`). False when
    /// no Nvim edits a text.
    pub(super) fn vim_pasted(&mut self, text: &str) -> bool {
        let Some(vim) = &mut self.vim else { return false };
        let mut out = Vec::new();
        vim.session.paste(text, &mut out);
        vim.process.send(&out);
        true
    }

    /// The app's undo or redo, while Nvim edits a text, is Nvim's.
    pub(super) fn vim_undo(&mut self, redo: bool) -> bool {
        let Some(vim) = &mut self.vim else { return false };
        let mut out = Vec::new();
        vim.session.command(if redo { "silent! redo" } else { "silent! undo" }, &mut out);
        vim.process.send(&out);
        true
    }

    /// A press at `at` on `here` at `zoom`: inside the grid it is Nvim's
    /// (true); outside, Nvim first takes the keys already sent, so the
    /// text the press ends is whole.
    pub(super) fn vim_press(
        &mut self,
        here: (&str, &str),
        at: Point,
        zoom: f64,
        shift: bool,
    ) -> bool {
        let Some(vim) = &mut self.vim else { return false };
        let Some(cell) = self.shapes.cell_at(here, at, REACH / zoom) else {
            vim.settle(&mut self.shapes);
            return false;
        };
        vim.pressed = true;
        let mut out = Vec::new();
        vim.session.mouse("press", shift, cell, &mut out);
        vim.process.send(&out);
        true
    }

    /// The pen moved to `at` (`lift`: and lifted) after a press inside the grid.
    pub(super) fn vim_drag(&mut self, at: Point, lift: bool) -> bool {
        let Some(vim) = self.vim.as_mut().filter(|vim| vim.pressed) else { return false };
        vim.pressed = !lift;
        let Some(here) = self.shapes.typing_on().map(|(c, p)| (c.to_string(), p.to_string()))
        else {
            return false;
        };
        if let Some(cell) = self.shapes.cell_at((&here.0, &here.1), at, f64::MAX) {
            let mut out = Vec::new();
            vim.session.mouse(if lift { "release" } else { "drag" }, false, cell, &mut out);
            vim.process.send(&out);
        }
        true
    }

    /// Whether a key's grid is shown and not yet painted: the next scene
    /// asks for the paint to be told.
    pub(super) fn vim_unpainted(&self) -> bool {
        self.vim.as_ref().is_some_and(|vim| !vim.unpainted.is_empty())
    }

    /// A paint showed Nvim's grid: each key it was the first to show says
    /// how long it took, in microseconds from the key: to the flush off
    /// the pipe, to the main thread, to the paint's end.
    pub(super) fn vim_painted(&mut self) {
        let Some(vim) = &mut self.vim else { return };
        let now = Instant::now();
        for (key, heard, taken) in std::mem::take(&mut vim.unpainted) {
            let us = |t: Instant| (t - key).as_micros();
            vim.say(&format!(
                "key {} {} {} gutter {}",
                us(heard),
                us(taken),
                us(now),
                vim.session.gutter
            ));
        }
    }
}
