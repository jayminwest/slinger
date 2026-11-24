//! The Nvim that edits the text being typed ([`crate::nvim`],
//! `backend::nvim`; docs/design/writing-column.md). [`Vim`] starts one when
//! a text opens ([`Vim::sync`]), hands it the keys and the pastes, and
//! takes its news ([`Vim::poll`]): each change of the buffer's lines, once
//! the buffer is confirmed, goes to the text, so the text ends, from
//! anywhere, with what Nvim holds; each grid Nvim flushed goes to the
//! scene, which shows it in place of the text ([`Shapes::set_grid`]).
//!
//! No way of ending loses the text. Every end waits, at most [`SETTLE`],
//! for Nvim to take the keys already sent ([`Vim::settle`]). A text ends
//! with the buffer as last confirmed: escape in normal mode, Nvim exiting
//! (`:q`, `ZZ`) or crashing, a press outside, `new-text`, the app
//! quitting. With nothing confirmed (Nvim missing, too old, broken by the
//! config, quitting or not ready within [`READY`]) the text keeps what it
//! had, the app's own editor types on, the flash says why, and no Nvim
//! starts again this run.
//!
//! While it edits, every key without command is Nvim's, but `new-text`'s;
//! the app's undo, redo, select all, copy and cut are Nvim's `undo`,
//! `redo`, `ggVG`, `"+y` and `"+d` (its clipboard is the system's), a
//! paste and a dictation are `nvim_paste`, and a press in the grid with
//! its drag and lift is Nvim's mouse (docs/design/writing-column.md, "The
//! keys").

use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::backend::Wake;
use crate::backend::nvim::{Process, Setup};
use crate::commands::{CMD, Chord};
use crate::config::Nvim;
use crate::layout::Point;
use crate::nvim::pack::Value;
use crate::nvim::{Heard, Session, keys};
use crate::scene::ink::Ink;
use crate::scene::shapes::Shapes;

/// The most a leave waits for Nvim to take the keys already sent.
const SETTLE: Duration = Duration::from_millis(250);
/// How long a new Nvim has to load its config and seed the buffer.
const READY: Duration = Duration::from_secs(2);
/// What every flash of a fallback ends with.
const FALLBACK: &str = "typing with slinger's editor ([editor] nvim)";

/// The Nvim of the text being typed, if one edits it, and how one starts.
pub(super) struct Vim {
    open: Option<Open>,
    /// How an Nvim starts: `None` while off, or after one failed to.
    setup: Option<Setup>,
    /// The config last followed.
    config: Option<Nvim>,
    /// What to flash, once.
    flash: Option<String>,
    /// How long a new Nvim has to be ready.
    ready_within: Duration,
    /// Whether the config may turn Nvim on: not in a driven window or a
    /// test, where only `SLINGER_NVIM` does.
    by_config: bool,
    wake: Wake,
}

/// One text's Nvim.
struct Open {
    /// [`Shapes::ends`] when it started: the text it edits.
    text: u64,
    process: Process,
    session: Session,
    started: Instant,
    /// Whether the button went down in the grid and is not up yet: the
    /// drag and the lift are Nvim's too.
    pressed: bool,
}

impl Open {
    /// A line of the trace, if one is kept.
    fn say(&mut self, line: &str) {
        self.process.say(line);
    }

    fn send(&mut self, out: &[u8]) {
        // An Nvim that is gone says so to the reader, which [`Vim::poll`] hears.
        self.process.send(out);
    }

    /// Takes one message, heard at `at`: a confirmed buffer goes to `shapes`.
    fn take(&mut self, message: &Value, at: Instant, shapes: &mut Shapes) -> Heard {
        let mut out = Vec::new();
        let heard = self.session.hear(message, &mut out);
        match &heard {
            Heard::Lines => {
                if let Some(text) = self.session.text() {
                    shapes.set_text(text);
                }
            }
            Heard::Flushed => shapes.set_grid(Some(Arc::new(self.session.grid().clone()))),
            Heard::Ready => {
                let us = (at - self.started).as_micros();
                self.say(&format!("ready_us {us}"));
            }
            Heard::Failed(why) => self.say(&format!("failed {why}")),
            _ => {}
        }
        self.send(&out);
        heard
    }
}

impl Vim {
    /// None started yet: the config decides ([`Vim::follow`]); Nvim's
    /// messages wake the main thread with `wake`.
    pub(super) fn new(wake: &Wake) -> Self {
        let (open, setup, config, flash) = (None, None, None, None);
        let (ready_within, by_config) = (READY, !cfg!(test));
        Vim { open, setup, config, flash, ready_within, by_config, wake: wake.clone() }
    }

    /// Only `SLINGER_NVIM` turns Nvim on from now on, not the config: a
    /// driven window types the same on any machine.
    pub(super) fn driven(&mut self) {
        (self.by_config, self.config) = (false, None);
    }

    /// Follows the config's `[editor] nvim` (and `SLINGER_NVIM`): an Nvim
    /// not found flashes once.
    pub(super) fn follow(&mut self, nvim: &Nvim) {
        if self.config.as_ref() == Some(nvim) {
            return;
        }
        self.config = Some(nvim.clone());
        match Setup::resolve(nvim, self.by_config, &|name| std::env::var_os(name)) {
            Ok(setup) => self.setup = setup,
            Err(why) => self.fail(&why),
        }
    }

    /// Starts Nvims as `setup` says, each with `ready_within` to be ready (a test's).
    #[cfg(test)]
    pub(super) fn set(&mut self, setup: Setup, ready_within: Duration) {
        (self.setup, self.config, self.ready_within) =
            (Some(setup), Some(Nvim::Auto), ready_within);
    }

    /// Whether an Nvim edits the text being typed.
    #[cfg(test)]
    pub(super) fn editing(&self) -> bool {
        self.open.is_some()
    }

    /// Whether that Nvim's buffer is confirmed.
    #[cfg(test)]
    pub(super) fn confirmed(&self) -> bool {
        self.open.as_ref().is_some_and(|open| open.session.text().is_some())
    }

    /// Kills the Nvim, as a crash would (a test's).
    #[cfg(test)]
    pub(super) fn crash(&mut self) {
        if let Some(open) = &mut self.open {
            open.process.crash();
        }
    }

    /// Whether an Nvim edits the texts typed from now on.
    pub(super) fn on(&self) -> bool {
        self.setup.is_some()
    }

    /// What to flash, once.
    pub(super) fn take_flash(&mut self) -> Option<String> {
        self.flash.take()
    }

    /// No Nvim from now on, this run: the flash says `why`.
    fn fail(&mut self, why: &str) {
        self.setup = None;
        self.flash = Some(format!("{why}: {FALLBACK}"));
        if let Some(mut open) = self.open.take() {
            open.say(&format!("fallback {why}"));
        }
    }

    /// Starts an Nvim for a text that opened, and lets go of the one whose
    /// text ended. With no Nvim to start, the app's own editor types on.
    pub(super) fn sync(&mut self, shapes: &Shapes) {
        if self.open.as_ref().is_some_and(|o| o.text != shapes.ends() || !shapes.editing()) {
            self.open = None;
        }
        if self.open.is_some() || !shapes.editing() {
            return;
        }
        let Some(setup) = &self.setup else { return };
        let started = Instant::now();
        match Process::start(setup, &self.wake) {
            Ok(process) => {
                let mut out = Vec::new();
                let text = shapes.typing_text().unwrap_or_default();
                let session = Session::start(&text, shapes.typing_new(), &mut out);
                let (text, pressed) = (shapes.ends(), false);
                let mut open = Open { text, process, session, started, pressed };
                open.send(&out);
                open.say(&format!("spawned_us {}", started.elapsed().as_micros()));
                self.open = Some(open);
            }
            Err(why) => self.fail(&format!("nvim did not start ({why})")),
        }
    }

    /// Takes Nvim's news into the text being typed. An Nvim that ended
    /// (`:q`, `ZZ`, a crash) with its buffer confirmed ends the text (to
    /// `ink`); one that ended, broke or was not ready in time before then
    /// leaves the text to the app's own editor. Returns whether the text
    /// changed or ended.
    pub(super) fn poll(&mut self, shapes: &mut Shapes, ink: &mut Ink) -> bool {
        self.sync(shapes);
        let Some(open) = &mut self.open else { return false };
        let (messages, gone) = open.process.heard();
        let mut changed = false;
        let mut failed = None;
        for (message, at) in messages {
            match open.take(&message, at, shapes) {
                Heard::Lines => changed = true,
                Heard::Old(version) => failed = Some(format!("nvim {version} is older than 0.10")),
                Heard::Broken(why) => {
                    failed = Some(format!("nvim could not open the text ({why})"));
                }
                _ => {}
            }
        }
        let confirmed = open.session.text().is_some();
        if failed.is_none() && !open.session.ready() && open.started.elapsed() > self.ready_within {
            failed = Some(format!("nvim was not ready in {} s", self.ready_within.as_secs()));
        }
        if failed.is_none() && gone && !confirmed {
            failed = Some("nvim quit before it was ready".into());
        }
        if let Some(why) = failed {
            self.fail(&why);
            shapes.set_grid(None);
            return changed;
        }
        if gone {
            open.say("gone");
            self.open = None;
            return shapes.end(ink) || changed;
        }
        changed
    }

    /// Waits, at most [`SETTLE`], until Nvim has taken every key sent so
    /// far, so its mode and the text are current. Call it before anything
    /// that may end the text.
    pub(super) fn settle(&mut self, shapes: &mut Shapes) {
        let Some(open) = &mut self.open else { return };
        let mut out = Vec::new();
        open.session.barrier(&mut out);
        open.send(&out);
        let until = Instant::now() + SETTLE;
        while let Some((message, at)) =
            open.process.wait(until.saturating_duration_since(Instant::now()))
        {
            if open.take(&message, at, shapes) == Heard::Barrier {
                break;
            }
        }
    }

    /// A key press, as its chord: `None` when no Nvim edits a text. Escape
    /// in normal mode ends the text (to `ink`); a named key or a control
    /// chord goes to Nvim, and cmd+A selects the whole buffer. Any other
    /// chord is not Nvim's (false): `new-text`'s, a key that types (it
    /// comes again as its text) and a command chord (the app's).
    pub(super) fn key(&mut self, chord: Chord, shapes: &mut Shapes, ink: &mut Ink) -> Option<bool> {
        self.sync(shapes);
        self.open.as_ref()?;
        // `new-text`'s key ends the text and starts the next: the app's.
        if shapes.next_key() == Some(chord) {
            return Some(false);
        }
        if chord == Chord::new(0, '\u{1b}') {
            self.settle(shapes);
            let open = self.open.as_mut()?;
            if open.session.normal() {
                open.say("leave escape");
                self.open = None;
                shapes.end(ink);
                return Some(true);
            }
        }
        // Select all: the whole buffer, linewise, from any mode.
        let keys = if chord == Chord::new(CMD, 'a') {
            "<C-\\><C-N>ggVG".to_string()
        } else {
            let Some(keys) = keys::chord(chord) else { return Some(false) };
            keys
        };
        Some(self.input(&keys, shapes))
    }

    /// Types `keys` (Nvim's notation). False when no Nvim edits a text.
    fn input(&mut self, keys: &str, shapes: &Shapes) -> bool {
        self.sync(shapes);
        let Some(open) = &mut self.open else { return false };
        let mut out = Vec::new();
        open.session.input(keys, &mut out);
        open.send(&out);
        true
    }

    /// Typed `text` goes to Nvim. False when no Nvim edits a text.
    pub(super) fn typed(&mut self, text: &str, shapes: &Shapes) -> bool {
        self.input(&keys::text(text), shapes)
    }

    /// The app's undo (`redo`: redo) is Nvim's while it edits a text, in
    /// turn with the keys. False when none does.
    pub(super) fn undo(&mut self, redo: bool, shapes: &Shapes) -> bool {
        self.input(if redo { "<Cmd>silent! redo<CR>" } else { "<Cmd>silent! undo<CR>" }, shapes)
    }

    /// Whether Nvim selects something to copy (visual mode): `None` when
    /// no Nvim edits a text.
    pub(super) fn selects(&self) -> Option<bool> {
        self.open.as_ref().map(|open| open.session.visual())
    }

    /// Copies what visual mode selects to Nvim's clipboard, the system's
    /// (`"+y`); with `cut`, takes it out too (`"+d`). False when no Nvim
    /// edits a text.
    pub(super) fn copy(&mut self, cut: bool, shapes: &Shapes) -> bool {
        self.input(if cut { "\"+d" } else { "\"+y" }, shapes)
    }

    /// A press at `at` on `here`, a canvas and its page, with `shift`
    /// held: within `reach` of the grid it is Nvim's mouse at that cell,
    /// and so are the drag and the lift after it (true). False, and
    /// nothing is sent, anywhere else.
    pub(super) fn press(
        &mut self,
        here: (&str, &str),
        (at, reach): (Point, f64),
        shift: bool,
        shapes: &mut Shapes,
    ) -> bool {
        let Some(open) = &mut self.open else { return false };
        let Some(cell) = shapes.cell_at(here, at, reach) else { return false };
        open.pressed = true;
        let mut out = Vec::new();
        open.session.mouse("press", shift, cell, &mut out);
        open.send(&out);
        // Nvim takes a drag queued with its press as no drag: it takes the press first.
        self.settle(shapes);
        true
    }

    /// The pen moved to `at` (`lift`: and lifted) after a press
    /// [`Vim::press`] took: it drags in the grid, to the nearest cell.
    /// False when no such press is down.
    pub(super) fn drag(&mut self, at: Point, lift: bool, shapes: &Shapes) -> bool {
        let Some(open) = self.open.as_mut().filter(|open| open.pressed) else { return false };
        open.pressed = !lift;
        let here = shapes.typing_on();
        if let Some(cell) = here.and_then(|here| shapes.cell_at(here, at, f64::MAX)) {
            let mut out = Vec::new();
            open.session.mouse(if lift { "release" } else { "drag" }, false, cell, &mut out);
            open.send(&out);
        }
        true
    }

    /// Pasted `text` goes in at Nvim's cursor (`nvim_paste`). False when
    /// no Nvim edits a text.
    pub(super) fn pasted(&mut self, text: &str, shapes: &Shapes) -> bool {
        self.sync(shapes);
        let Some(open) = &mut self.open else { return false };
        let mut out = Vec::new();
        open.session.paste(text, &mut out);
        open.send(&out);
        true
    }
}

#[cfg(test)]
mod tests;
