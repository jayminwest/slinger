//! The sling's flow, apart from the pages it slings: the sling being made
//! ([`crate::scene::sling`]) and the thread that sends it ([`Slinger`]),
//! what became of the last one ([`crate::scene::slung`]), the words a sling
//! with none sends (`[sling] prompt`), dictation into the open entry
//! ([`voice`]), the feedback entry ([`notes`]) and each open workspace's
//! agent as the sidebar last heard. [`SlingFlow`] hides all of these: the
//! canvases hand it the pages it needs (each [`Open`]) and the ink a sling
//! waits for, and ask it what to show. It starts three threads: the
//! slinger, Flick's and the feedback's. Runs on the main thread and never
//! waits.

mod notes;
mod voice;

use std::time::Instant;

use slinger_control::Client;

use super::Open;
use crate::backend::{Flick, Heard, Render, Slinger, Wake};
use crate::commands::{Chord, Command, Keymap};
use crate::config::{Prompt, Theme};
use crate::layout::{Point, Rect};
use crate::scene::canvas::ItemId;
use crate::scene::feedback::Context;
use crate::scene::ink::Ink;
use crate::scene::pages::Pages;
use crate::scene::select::Selection;
use crate::scene::sidebar::{Agent, Sidebar};
use crate::scene::sling::{Answered, Ask, Key, Profile, Sling, pen};
use crate::scene::slung::{Marks, Slung, Tapped, View};
use crate::scene::taps::Target;
use crate::scene::{Label, Scene};

/// The sling, its slinger, what became of it, its prompt, its dictation,
/// the feedback entry beside it, and the agents it goes to.
pub(super) struct SlingFlow {
    sling: Sling,
    slinger: Slinger,
    /// What became of the last sling.
    slung: Slung,
    /// The words a sling the user wrote none for sends.
    prompt: String,
    /// Speaking the sling's prompt through Flick.
    voice: voice::Voice,
    /// The feedback being left.
    notes: notes::Notes,
    /// Each open workspace's canvas and its agent, as the sidebar last
    /// heard: for the buttons, the footers and `sling-now`.
    agents: Option<Vec<(String, Option<Agent>)>>,
}

impl SlingFlow {
    /// Nothing slung yet: slings and notes go through `daemon`, dictation
    /// to Flick's socket, and each thread's news wakes the main thread
    /// with `wake`.
    pub(super) fn start(daemon: &Client, wake: &Wake) -> Self {
        Self {
            sling: Sling::default(),
            slinger: Slinger::start(daemon.clone(), None, wake),
            slung: Slung::default(),
            prompt: Prompt::default().text().to_string(),
            voice: voice::Voice::new(Flick::socket(), wake),
            notes: notes::Notes::new(daemon.clone(), wake),
            agents: None,
        }
    }

    /// Renders the page under new ink with `render` for every sling from
    /// now on, through `daemon`.
    pub(super) fn rendering(&mut self, daemon: &Client, render: Render, wake: &Wake) {
        let pdfs = crate::backend::pdf::cache();
        self.slinger = Slinger::start(daemon.clone(), Some((render, pdfs)), wake);
    }

    /// Slings and saves notes through `daemon` from now on.
    pub(super) fn reconnect(&mut self, daemon: &Client) {
        self.slinger.reconnect(daemon.clone());
        self.notes.reconnect(daemon.clone());
    }

    /// Takes the config's `[sling] prompt`.
    pub(super) fn set_prompt(&mut self, prompt: &Prompt) {
        prompt.text().clone_into(&mut self.prompt);
    }

    /// Each open workspace's canvas and its agent, as the sidebar last
    /// heard (`None`: nothing heard).
    pub(super) fn set_agents(&mut self, agents: Option<Vec<(String, Option<Agent>)>>) {
        self.agents = agents;
    }

    /// The agent of the workspace whose canvas is `canvas`; `None` when it
    /// has none, or before the sidebar heard.
    pub(super) fn agent_of(&self, canvas: &str) -> Option<&Agent> {
        super::slung::agent_in(self.agents.as_deref(), canvas)
    }

    /// Why a sling of `canvas` cannot go ([`super::slung::unslingable`]).
    pub(super) fn unslingable(&self, canvas: &str) -> Option<&'static str> {
        super::slung::unslingable(self.agents.as_deref(), canvas)
    }

    /// Whether the sling's entry is open.
    pub(super) fn slinging(&self) -> bool {
        self.sling.is_typing()
    }

    /// Whether no sling is open or on its way.
    pub(super) fn idle(&self) -> bool {
        self.sling.canvas().is_none()
    }

    /// Whether the feedback entry is open.
    pub(super) fn noting(&self) -> bool {
        self.notes.is_typing()
    }

    /// Whether the sling's entry or the feedback's is open: it takes the keys.
    pub(super) fn typing(&self) -> bool {
        self.sling.is_typing() || self.notes.is_typing()
    }

    /// Whether `at`, a point in the window, is on the sling's entry or the
    /// feedback's.
    pub(super) fn covers(&self, at: Point) -> bool {
        self.sling.covers(at) || self.notes.covers(at)
    }

    /// Opens the sling's entry ([`Sling::open`]).
    pub(super) fn open(
        &mut self,
        of: (&Selection, Option<ItemId>),
        around: Rect,
        within: Rect,
    ) -> bool {
        self.sling.open(of, around, within)
    }

    /// Slings at once, with no entry ([`Sling::send_now`]).
    pub(super) fn send_now(
        &mut self,
        of: (&Selection, Option<ItemId>),
        prompt: (&str, Profile),
        around: Rect,
        within: Rect,
    ) -> bool {
        self.sling.send_now(of, prompt, around, within)
    }

    /// Sends the open entry under `profile` ([`Profile::Running`]: the one
    /// it has), as return does; the entry closes.
    pub(super) fn send(&mut self, profile: Profile) {
        self.sling.send_under(profile);
        self.entry_closed();
    }

    /// Closes the sling's entry and the feedback's. False when neither was
    /// open.
    pub(super) fn cancel(&mut self) -> bool {
        self.sling.cancel() | self.notes.cancel()
    }

    /// Closes the sling's entry. False when it was closed.
    pub(super) fn cancel_sling(&mut self) -> bool {
        self.sling.cancel()
    }

    /// An entry closed: a dictation still going is cancelled.
    pub(super) fn entry_closed(&mut self) {
        self.voice.closed();
    }

    /// A key press while the feedback entry is open: return sends and
    /// escape cancels (true: draw). `None` while it is closed.
    pub(super) fn feedback_key(&mut self, chord: Chord) -> Option<bool> {
        let closed = match self.notes.key(chord)? {
            Key::Send => {
                self.notes.send();
                true
            }
            Key::Cancel => self.notes.cancel(),
            Key::Type => return Some(false),
        };
        self.entry_closed();
        Some(closed)
    }

    /// A key press while the sling's entry is open: return sends (taking
    /// what is new at once, the pages being `open` and the ink `ink`) and
    /// escape cancels (true: draw). False for any other key, and for every
    /// key while it is closed.
    pub(super) fn sling_key(&mut self, chord: Chord, open: &[Open], ink: &Ink) -> bool {
        let closed = match self.sling.key(chord) {
            Some(Key::Send) => {
                self.sling.send();
                self.poll(open, ink);
                true
            }
            Some(Key::Cancel) => self.sling.cancel(),
            Some(Key::Type) | None => return false,
        };
        self.entry_closed();
        closed
    }

    /// The feedback entry's text, else the sling's, is now `text`.
    pub(super) fn typed(&mut self, text: &str) {
        if !self.notes.typed(text) {
            self.sling.typed(text);
        }
    }

    /// The `dictate` command with an entry open: starts Flick's dictation
    /// into it, or stops the one going.
    pub(super) fn dictate(&mut self) {
        self.voice.toggle();
    }

    /// Takes Flick's answers. Returns whether anything shown changed.
    pub(super) fn poll_dictation(&mut self) -> bool {
        self.voice.poll()
    }

    /// Asks the Flick on `socket` from now on: a test's stand-in.
    #[cfg(test)]
    pub(super) fn listen_to(&mut self, socket: std::path::PathBuf) {
        self.voice = voice::Voice::new(socket, &Wake::none());
    }

    /// Opens the feedback entry at `frame` for a note on `context`. False
    /// while a note saves.
    pub(super) fn open_feedback(&mut self, frame: Rect, context: Context) -> bool {
        self.notes.open(frame, context)
    }

    /// Whether the feedback entry lists the resolved notes too.
    pub(super) fn list_all_feedback(&mut self, all: bool) {
        self.notes.list_all(all);
    }

    /// Resolves the recent note listed `index`th under the feedback entry.
    pub(super) fn resolve_feedback(&mut self, index: usize) -> bool {
        self.notes.resolve(index)
    }

    /// What to flash once a note is saved, once.
    pub(super) fn take_flash(&mut self) -> Option<String> {
        self.notes.flash()
    }

    /// Takes the daemon's answers to the notes. Returns whether anything
    /// shown changed.
    pub(super) fn poll_feedback(&mut self) -> bool {
        self.notes.poll()
    }

    /// Takes the daemon's answers to the sling and the feedback, and what
    /// it says of the agent since, and sends a waiting sling once `ink` of
    /// its canvas is in the daemon; `open` are the pages followed. Returns
    /// whether anything shown changed.
    pub(super) fn poll(&mut self, open: &[Open], ink: &Ink) -> bool {
        let changed = self.poll_feedback() | self.poll_slung(open);
        // A lost daemon would hold the ink, and the sling, for ever: the
        // sling goes and says so.
        if let Some(ask) = self.sling.ready(|c| ink.settled(c) || ink.problem().is_some()) {
            self.going(ask, open);
        }
        changed
    }

    /// Sends `ask`, now ready, with the words of `[sling] prompt` when it
    /// has none, and follows it from where it was made.
    fn going(&mut self, mut ask: Ask, open: &[Open]) {
        if ask.prompt.trim().is_empty() {
            ask.prompt.clone_from(&self.prompt);
        }
        let newest = newest(open, &ask);
        let under = match &ask.profile {
            Profile::Picked(picked) => picked.clone(),
            Profile::Running => self.agent_of(&ask.canvas).and_then(|a| a.profile.clone()),
        };
        self.slung.aim((&ask.canvas, &ask.page), under.as_deref(), newest, Instant::now());
        self.slinger.send(ask);
    }

    /// Takes what the slinger's thread heard, the agent's items on the
    /// sling's page, and the time. Returns whether anything shown changed.
    fn poll_slung(&mut self, open: &[Open]) -> bool {
        let now = Instant::now();
        let mut changed = self.landed(open);
        for heard in self.slinger.heard() {
            changed = true;
            match heard {
                Heard::Went(agent, tokens) => {
                    self.sling.answered(Ok(()));
                    self.slung.went(&agent, tokens, now);
                }
                Heard::Failed(problem) => match self.sling.answered(Err(problem.clone())) {
                    Answered::Reopened => self.slung.forget(),
                    Answered::Closed => drop(self.slung.failed(problem, now)),
                    Answered::Not => {}
                },
                Heard::Lost(problem) => drop(self.slung.failed(problem, now)),
                Heard::Progress(progress) => drop(self.slung.heard(&progress, now)),
            }
        }
        self.slung.tick(now) || changed
    }

    /// Hands the items of the sling's page in `open` to the pill, each with
    /// who made it. Returns whether its answer's items changed.
    fn landed(&mut self, open: &[Open]) -> bool {
        let Some((canvas, page)) = self.slung.watching() else { return false };
        let (canvas, page) = (canvas.to_string(), page.to_string());
        let Some(mirror) = mirror_of(open, &canvas, &page) else { return false };
        let items = mirror.items().iter().enumerate();
        let items = items.filter_map(|(at, i)| Some((i.id, i.shape.bounds()?, mirror.by(at))));
        self.slung.landed((&canvas, &page), items)
    }

    /// A press on a canvas: an ended sling's pill goes.
    pub(super) fn pressed(&mut self) {
        self.slung.pressed();
    }

    /// What became of the last sling, once, for the flash.
    pub(super) fn flash(&mut self) -> Option<String> {
        self.slung.flash()
    }

    /// A tap on the pill or the answer under it, the canvas panes being
    /// `views`.
    pub(super) fn tap(&mut self, views: &[View]) -> Tapped {
        self.slung.tapped(views)
    }

    /// Lays the sling over `scene` in `theme`'s colours: its entry, to the
    /// agent `sidebar` names for its canvas; or its line while it goes, or
    /// why it failed; the feedback entry or `asks`' new name's in its
    /// place while that is open, and the feedback's lines; whichever entry
    /// shows, with `keys`' dictate key; and what dictation says.
    pub(super) fn show(
        &self,
        scene: &mut Scene,
        (theme, keys): (&Theme, &Keymap),
        sidebar: &Sidebar,
        asks: &Pages,
    ) {
        let agent = self.sling.canvas().and_then(|canvas| sidebar.agent(canvas));
        let dictate = keys.chord(Command::Dictate);
        let entry = self.sling.entry(theme, agent, dictate);
        let (note, lines) = self.notes.shown(theme);
        let other = note.or_else(|| asks.entry(theme)).map(|e| e.dictated(dictate));
        scene.entry = other.or(entry).map(Box::new);
        scene.labels.extend(self.sling.labels(theme));
        scene.labels.extend(lines);
        scene.labels.extend(self.dictation_labels(theme, asks));
    }

    /// What dictation says under the open entry (`asks`' new name's among
    /// them), in `theme`'s colours.
    fn dictation_labels(&self, theme: &Theme, asks: &Pages) -> Vec<Label> {
        let lines = self.voice.lines(theme);
        if self.notes.is_typing() {
            self.notes.under(theme, lines)
        } else if asks.entering() {
            asks.under(theme, lines)
        } else {
            self.sling.under(theme, lines)
        }
    }

    /// What the window draws for the sling, in `theme`'s colours, after
    /// `buttons` (the selection's, if any): the pen's buttons beside the
    /// open entry (a note's, `asks`' new name's, the sling's), the "✓" that
    /// resolves each recent note under a note's, and the pill over the
    /// canvas panes `views`.
    pub(super) fn marks(
        &self,
        (labels, targets): (Vec<Label>, Vec<Target>),
        asks: &Pages,
        theme: &Theme,
        views: &[View],
    ) -> Marks {
        let (pen, tapped) = self.entry_buttons(theme, asks);
        let (ticks, ticked) = self.notes.ticks(theme);
        let (mut labels, mut targets) =
            ([labels, pen, ticks].concat(), [targets, tapped, ticked].concat());
        let mut marks = self.slung.marks(theme, views, Instant::now());
        labels.append(&mut marks.labels);
        targets.append(&mut marks.targets);
        Marks { labels, targets, ..marks }
    }

    /// The pen's buttons beside the open entry ([`pen`]), in `theme`'s
    /// colours: their labels and their targets.
    fn entry_buttons(&self, theme: &Theme, asks: &Pages) -> (Vec<Label>, Vec<Target>) {
        let note = self.notes.shown(theme).0.or_else(|| asks.entry(theme));
        let entry = note.or_else(|| self.sling.entry(theme, None, None));
        let Some((frame, done)) = entry.and_then(|e| Some((e.frame, e.done?))) else {
            return (Vec::new(), Vec::new());
        };
        pen::buttons(frame, done, self.voice.listening(), theme)
    }

    /// The sling being made: a test's look at it.
    #[cfg(test)]
    pub(super) fn sling(&self) -> &Sling {
        &self.sling
    }

    /// The words a sling with none sends: a test's look at them.
    #[cfg(test)]
    pub(super) fn prompt(&self) -> &str {
        &self.prompt
    }
}

/// The mirror of `canvas`'s page `page` among `open`, if it has arrived.
fn mirror_of<'a>(open: &'a [Open], canvas: &str, page: &str) -> Option<&'a crate::mirror::Mirror> {
    let open = open.iter().filter(|o| o.canvas == canvas);
    open.filter_map(|o| o.mirror.as_ref()).find(|m| m.page() == page)
}

/// The newest item on the page of `ask` in `open` now: the agent's items
/// after it are its answer.
fn newest(open: &[Open], ask: &Ask) -> ItemId {
    let mirror = mirror_of(open, &ask.canvas, &ask.page);
    let items = mirror.map(crate::mirror::Mirror::items).unwrap_or_default();
    items.iter().map(|i| i.id).max().unwrap_or(ItemId(0))
}
