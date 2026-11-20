//! The sling from the window: what the user is slinging
//! ([`crate::scene::sling`]) tied to the selection it sends and the ink it
//! waits for, and the `sling` and `dictate` commands. The sling itself, the
//! thread that sends it and the dictation are [`super::flow`]'s; this
//! hands it the pages and the ink. Runs on the main thread and never waits.
//! A sling waits until every change of its canvas made before it is written
//! (`Editing::settled`); `sling_now` is the one way in for a sling with no typing.
//!
//! The ink's problem (a daemon that cannot be reached) does not hold a
//! sling back (`SlingFlow::poll`).

use super::Canvases;
use crate::backend::Render;
use crate::commands::{Chord, Command, Keymap};
use crate::config::Theme;
use crate::layout::{Point, Rect, Viewport};
use crate::panes::Panes;
use crate::scene::Scene;
use crate::scene::canvas::ItemId;
use crate::scene::select::Selection;
use crate::scene::sidebar::Sidebar;
use crate::scene::sling::{Key, Profile};

impl Canvases {
    /// Renders the page under new ink with `render` for every sling from
    /// now on (`platform::pdf::backdrop`); without it, ink goes on white.
    #[must_use]
    pub fn rendering(mut self, render: Render) -> Self {
        self.flow.rendering(&self.daemon, render, &self.wake);
        self
    }

    /// The `sling` command: opens the entry at the selection, when the
    /// focused pane of `panes` laid in `area` shows it; else beside the
    /// first button on that pane's page, to sling what is new there. False
    /// when it shows neither, a sling is on its way, or feedback is open. A
    /// page's new name's entry closes, and once it opens a text being typed
    /// ends and goes to the daemon, as the feedback entry ends it: the
    /// entry's caret is the only one, and the text is on the page the sling
    /// waits for (slinger-cd5a58).
    pub fn open_sling(&mut self, panes: &Panes, area: Rect) -> bool {
        if self.flow.noting() {
            return false;
        }
        self.asks.cancel_entry();
        let Some((selection, around, within)) = self.sling_target(panes, area, false) else {
            return false;
        };
        let inside = self.follow.sling_box(panes.focus(), &selection);
        let opened = self.flow.open((&selection, inside), around, within);
        if opened && self.editing.end_text() {
            self.send();
        }
        opened
    }

    /// Slings `selection` (no ids: what is new on its page), of the box
    /// given if any, with `prompt`
    /// under `profile`, without the entry; its line, and a failure's entry, stand under
    /// `around` inside `within`, in the window. For a sling that needs no
    /// typing: a tap on a button. False while a sling is on its way.
    pub(super) fn sling_now(
        &mut self,
        of: (&Selection, Option<ItemId>),
        prompt: (&str, Profile),
        around: Rect,
        within: Rect,
    ) -> bool {
        let sent = self.flow.send_now(of, prompt, around, within);
        self.poll_sling();
        sent
    }

    /// The `sling` and `dictate` commands on the focused pane of `panes`
    /// laid in `area`. `sling` opens the entry at the selection. `dictate`
    /// opens it too when no entry is open, then starts Flick's dictation
    /// into the open entry (the sling's, the feedback's or a new name's),
    /// or stops the one going, after which Flick types what it heard into
    /// the entry. False when no entry is open and the sling's cannot open
    /// (no selection there, or a sling on its way).
    pub fn sling_command(&mut self, command: Command, panes: &Panes, area: Rect) -> bool {
        if command != Command::Dictate {
            return self.open_sling(panes, area);
        }
        if !self.typing() && !self.open_sling(panes, area) {
            return false;
        }
        self.flow.dictate();
        true
    }

    /// Whether an entry is open (the sling's, the feedback's or a page's new
    /// name's): it takes the keys.
    pub fn typing(&self) -> bool {
        self.flow.typing() || self.asks.entering()
    }

    /// Closes whichever entry is open: one shows at a time.
    pub(super) fn close_entries(&mut self) {
        if self.flow.cancel() | self.asks.cancel_entry() {
            self.flow.entry_closed();
        }
    }

    /// A key press while the entry is open: return sends and escape
    /// cancels (true: draw), and either cancels a dictation going. False
    /// for any other key, which is the entry's own and goes on to it, and
    /// for every key while it is closed.
    pub fn sling_key(&mut self, chord: Chord) -> bool {
        if let Some(closed) = self.flow.feedback_key(chord) {
            return closed;
        }
        if self.asks.entering() {
            let answer = match Key::of(chord) {
                Key::Send => "\r",
                Key::Cancel => "\u{1b}",
                Key::Type => return false,
            };
            if let Some((canvas, op)) = self.asks.typed(answer) {
                self.change_pages(canvas, op);
            }
            self.seek_sought();
            self.flow.entry_closed();
            return true;
        }
        self.flow.sling_key(chord, self.follow.open(), self.editing.ink())
    }

    /// The entry's text is now `text`. Nothing more to draw (false): the
    /// entry shows it already.
    pub fn sling_typed(&mut self, text: &str) -> bool {
        if self.asks.entering() {
            self.asks.named(text);
        } else {
            self.flow.typed(text);
        }
        false
    }

    /// Whether `at`, a point in the window, is on the open entry: a press
    /// there is the entry's.
    pub fn sling_covers(&self, at: Point) -> bool {
        self.flow.covers(at) || self.asks.covers(at)
    }

    /// Lays the sling over `scene` in `theme`'s colours: its entry, to the
    /// agent `sidebar` names for its canvas; or its line while it goes, or
    /// why it failed; the feedback entry or a page's new name's in its
    /// place while that is open, and the feedback's lines; whichever entry
    /// shows, with `keys`' dictate key; and what dictation says.
    pub fn show_sling(&self, scene: &mut Scene, theme: &Theme, keys: &Keymap, sidebar: &Sidebar) {
        self.flow.show(scene, (theme, keys), sidebar, &self.asks);
    }

    /// Takes the daemon's answers to the sling and the feedback, and what
    /// it says of the agent since, and sends a waiting sling once the ink
    /// of its canvas is in the daemon. Returns whether anything shown
    /// changed.
    pub(super) fn poll_sling(&mut self) -> bool {
        self.flow.poll(self.follow.open(), self.editing.ink())
    }

    /// The selection, when the focused pane of `panes` laid in `area` shows
    /// it, with its box and the pane's frame, in the window.
    pub(super) fn selection_in(
        &self,
        panes: &Panes,
        area: Rect,
    ) -> Option<(&Selection, Rect, Rect)> {
        let (selection, pane) = (self.selection()?, panes.focus());
        let placed = panes.place(area).into_iter().find(|(id, _, _)| *id == pane);
        let frame =
            crate::scene::status::content(placed.filter(|_| self.shows_selection(panes, pane))?.2);
        let shown = self.follow.pane_shown(self.editing.ink(), panes, pane);
        let bounds = self.editing.selection_bounds(&shown)?;
        let view = self.follow.view_of(pane);
        let on = view.map_or_else(Viewport::start, |v| v.viewport).rect_to_screen(bounds);
        Some((selection, Rect { x: on.x + frame.x, y: on.y + frame.y, ..on }, frame))
    }
}

#[cfg(test)]
mod tests {
    use slinger_control::Client;

    use super::*;
    use crate::backend::testing::{
        delivered, fake_claude, fake_flick, ready, user_stroke, wait_until,
    };
    use crate::backend::tests::{open_notes, with_store};
    use crate::config::{Config, Rgb};
    use crate::layout::{CANVAS_MARGIN, WINDOW};
    use crate::panes::{Kind, Pane};
    use crate::scene::ink::Pen;
    use crate::scene::tools::Tool;

    /// A press of the pen's tip.
    const PEN: Pen = Pen {
        at: Point { x: 0.0, y: 0.0 },
        when: 0.0,
        touch: false,
        eraser: false,
        shift: false,
        tablet: false,
    };

    fn notes() -> Panes {
        Panes::new(Pane { kind: Kind::Canvas, name: "notes".into() })
    }

    /// What the window shows of the sling: its entry and its lines.
    fn shown(canvases: &Canvases, panes: &Panes) -> Scene {
        let theme = Config::default().theme;
        let side = crate::scene::sidebar::Sidebar::new(false);
        let (uis, notes) = (crate::scene::Uis::new(), crate::scene::Notes::new());
        let mut scene =
            crate::scene::build(WINDOW, panes, &theme, &[], &side, Vec::new(), &uis, &notes);
        scene.labels.clear();
        canvases.show_sling(&mut scene, &theme, &Keymap::default(), &side);
        scene
    }

    fn texts(scene: &Scene) -> Vec<&str> {
        scene.labels.iter().map(|l| l.text.as_str()).collect()
    }

    #[test]
    fn with_nothing_selected_nothing_opens_or_goes_and_keys_go_on() {
        let mut canvases = Canvases::new(Client::new("/nonexistent/x.sock"));
        let (panes, area) = (notes(), WINDOW.rect());
        assert!(!canvases.open_sling(&panes, area));
        assert!(!canvases.typing());
        assert!(!canvases.sling_key(Chord::new(0, '\r')), "closed: the keymap's");
        assert!(!canvases.sling_typed("ignored"));
        assert!(!canvases.poll_sling());
        assert!(!canvases.sling_covers(Point { x: 1.0, y: 1.0 }));
        let scene = shown(&canvases, &panes);
        assert_eq!((scene.entry, scene.labels.len()), (None, 0));
    }

    /// Waits for `notes` to arrive in `canvases`, then lassoes all of its
    /// one stroke.
    fn select_the_stroke(canvases: &mut Canvases, panes: &Panes) {
        let (area, theme) = (WINDOW.rect(), Config::default().theme);
        wait_until(|| {
            canvases.poll();
            !canvases.show(panes, area, &theme).is_empty()
        });
        let pen = |x, y| Pen { at: Point { x, y }, ..PEN };
        let lasso = (Tool::Lasso, Rgb { r: 0, g: 0, b: 0 });
        assert!(canvases.pen_down(panes, (panes.focus(), area), pen(50.0, 50.0), lasso));
        for (x, y) in [(200.0, 50.0), (200.0, 200.0), (50.0, 200.0)] {
            canvases.pen_drag(panes, pen(x, y), Some((panes.focus(), area)));
        }
        canvases.pen_up(panes, pen(50.0, 50.0), Some((panes.focus(), area)));
        assert!(canvases.selection().is_some());
    }

    /// Waits until the sling is done with: its entry closed and no line of
    /// it left, so it went. Any poll's news is not that: other news comes too.
    fn gone(canvases: &mut Canvases) {
        let theme = Config::default().theme;
        wait_until(|| {
            canvases.poll();
            !canvases.typing() && canvases.flow.sling().labels(&theme).is_empty()
        });
    }

    /// Opens the entry at the selection, types `prompt` and sends it.
    fn sling(canvases: &mut Canvases, panes: &Panes, prompt: &str) {
        assert!(canvases.open_sling(panes, WINDOW.rect()));
        let entry = shown(canvases, panes).entry.unwrap();
        let (x, y) = (entry.frame.x - CANVAS_MARGIN, entry.frame.y - CANVAS_MARGIN);
        assert!(y > 110.0 && x <= 100.0, "under the stroke: {entry:?}");
        assert_eq!(entry.title.label.text, "Sling", "no workspace list: no agent to name");
        let dictate = Keymap::default().chord(Command::Dictate).map(Chord::glyphs);
        let keys = format!("↩ sends · esc cancels · {} dictate", dictate.unwrap());
        assert_eq!(entry.keys.label.text, keys, "the live key, on the keys' one row");
        let on = Point { x: entry.frame.x + 1.0, y: entry.frame.y + 1.0 };
        assert!(canvases.sling_covers(on) && canvases.typing());
        assert!(!canvases.sling_key(Chord::new(0, 'w')), "a letter is the entry's");
        assert!(!canvases.sling_typed(prompt));
        assert!(canvases.sling_key(Chord::new(0, '\r')));
        assert_eq!(texts(&shown(canvases, panes)), [format!("slinging 1 item: {prompt}")]);
    }

    /// Whether the canvas pane draws a text's caret.
    fn has_caret(canvases: &mut Canvases, panes: &Panes) -> bool {
        let shown = canvases.show(panes, WINDOW.rect(), &Config::default().theme);
        let mut draws = shown.iter().flat_map(|pane| &pane.scene.draws);
        draws.any(|d| matches!(d.mark, crate::scene::canvas::Mark::Caret { .. }))
    }

    #[test]
    fn the_entry_ends_the_text_being_typed_so_its_caret_is_the_only_one() {
        let bin = tempfile::tempdir().unwrap();
        with_store(Some(&fake_claude(bin.path())), |daemon| {
            let home = open_notes(daemon);
            user_stroke(daemon, "notes");
            daemon.session_start("notes").unwrap();
            let (mut canvases, panes) = (Canvases::new(daemon.clone()), notes());
            select_the_stroke(&mut canvases, &panes);
            let here = (panes.focus(), WINDOW.rect());
            let pen = Pen { at: Point { x: 300.0, y: 300.0 }, ..PEN };
            assert!(canvases.pen_down(
                &panes,
                here,
                pen,
                (Tool::Text, Config::default().theme.text)
            ));
            canvases.pen_up(&panes, Pen { when: 0.1, ..pen }, Some(here));
            assert!(canvases.typed("h") && canvases.typed("i") && canvases.editing());
            assert!(has_caret(&mut canvases, &panes), "the text's caret, while it is typed");

            assert!(canvases.open_sling(&panes, WINDOW.rect()), "the selection stays");
            assert!(!canvases.editing(), "the text ended");
            assert!(!has_caret(&mut canvases, &panes), "only the entry's caret");
            assert!(!canvases.sling_typed("and this?"));
            assert!(!canvases.typed("x"), "the entry's, not the text's");
            assert!(canvases.sling_key(Chord::new(0, '\r')));
            gone(&mut canvases);
            let message = delivered(home.path(), "and this?");
            assert!(message.contains("selected=c1"), "{message}");
            let items = daemon.canvas_show("notes", None).unwrap().items;
            let texts: Vec<_> = items.iter().filter(|i| i["kind"] == "text").collect();
            assert_eq!(texts.len(), 1, "the typed text is on the page it slung: {items:?}");
            assert_eq!(texts[0]["text"], "hi");
            daemon.session_stop("notes").unwrap();
        });
    }

    #[test]
    fn a_lasso_s_selection_slings_shows_why_it_failed_then_reaches_the_agent() {
        let bin = tempfile::tempdir().unwrap();
        with_store(Some(&fake_claude(bin.path())), |daemon| {
            let home = open_notes(daemon);
            user_stroke(daemon, "notes");
            let (mut canvases, panes) = (Canvases::new(daemon.clone()), notes());
            select_the_stroke(&mut canvases, &panes);

            // No open workspace has the canvas: the sling cannot go.
            daemon.workspace_close("notes").unwrap();
            sling(&mut canvases, &panes, "what is this?");
            wait_until(|| canvases.poll() && canvases.typing());
            let failed = shown(&canvases, &panes);
            assert_eq!(failed.entry.as_ref().map(|e| e.text.as_str()), Some("what is this?"));
            let lines = texts(&failed);
            assert!(lines[0].starts_with("✗ "), "{lines:?}");

            daemon.workspace_open(Some(home.path()), None).unwrap();
            daemon.session_start("notes").unwrap();
            assert!(canvases.sling_key(Chord::new(0, '\r')), "return tries again");
            gone(&mut canvases);
            let went = shown(&canvases, &panes);
            assert_eq!((went.entry, went.labels.len()), (None, 0), "it went: nothing more shows");
            let message = delivered(home.path(), "what is this?");
            assert!(message.contains("selected=c1"), "{message}");

            assert!(canvases.open_sling(&panes, WINDOW.rect()));
            assert!(canvases.sling_key(Chord::new(0, '\u{1b}')), "escape cancels");
            assert!(!canvases.typing());
            let selection = canvases.selection().unwrap().clone();
            let around = Rect { x: 50.0, y: 50.0, width: 10.0, height: 10.0 };
            assert!(canvases.sling_now(
                (&selection, None),
                ("and now?", Profile::Running),
                around,
                WINDOW.rect()
            ));
            assert!(
                !canvases.sling_now(
                    (&selection, None),
                    ("again?", Profile::Running),
                    around,
                    WINDOW.rect()
                ),
                "one at a time"
            );
            gone(&mut canvases);

            assert!(canvases.open_sling(&panes, WINDOW.rect()));
            let pen = Pen { at: Point { x: 400.0, y: 400.0 }, ..PEN };
            let ink = (Tool::Pen, Rgb { r: 0, g: 0, b: 0 });
            assert!(canvases.pen_down(&panes, (panes.focus(), WINDOW.rect()), pen, ink));
            assert!(!canvases.typing(), "a press on the canvas closes the entry");
            daemon.session_stop("notes").unwrap();
        });
    }

    /// The keymap's `dictate` chord: while the entry is open it is the one
    /// key that goes on to the keymap.
    fn dictate_key() -> Chord {
        let keys = Keymap::default();
        let chord = Chord::new(crate::commands::CMD | crate::commands::SHIFT, '\r');
        assert_eq!(keys.command(chord), Some(Command::Dictate));
        chord
    }

    /// Dictates into an entry that opens at the selection: Flick starts,
    /// then stops.
    fn dictate_and_stop(canvases: &mut Canvases, panes: &Panes) {
        let (keys, area) = (Keymap::default(), WINDOW.rect());
        assert!(canvases.sling_command(Command::Dictate, panes, area));
        assert!(canvases.typing(), "the entry opened");
        assert!(!canvases.takes(dictate_key(), &keys), "the dictate key reaches the keymap");
        assert!(canvases.takes(Chord::new(0, 'w'), &keys));
        let cmd = |key| Chord::new(crate::commands::CMD, key);
        assert!(canvases.takes(Chord::new(0, '\u{7f}'), &keys), "delete is the field's");
        assert!(canvases.takes(cmd('v'), &keys), "paste is the field's");
        assert!(canvases.takes(cmd('z'), &keys), "undo is the field's");
        assert!(!canvases.takes(cmd('q'), &keys), "cmd+Q reaches the keymap");
        assert!(!canvases.takes(cmd('k'), &keys), "the palette's key too");
        let listening = ["● Flick is listening: dictate again when done"];
        assert_eq!(texts(&shown(canvases, panes)), listening);
        wait_until(|| canvases.flow.poll_dictation());
        assert!(canvases.sling_command(Command::Dictate, panes, area), "again: stop");
        assert_eq!(texts(&shown(canvases, panes)), ["Flick is writing it down…"]);
        wait_until(|| canvases.flow.poll_dictation());
        assert_eq!(shown(canvases, panes).labels.len(), 0);
    }

    #[test]
    fn dictate_opens_the_entry_starts_and_stops_flick_and_the_spoken_text_slings() {
        let (bin, flick) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let (socket, heard) = fake_flick(flick.path(), ready);
        with_store(Some(&fake_claude(bin.path())), |daemon| {
            let home = open_notes(daemon);
            user_stroke(daemon, "notes");
            daemon.session_start("notes").unwrap();
            let (mut canvases, panes) = (Canvases::new(daemon.clone()), notes());
            canvases.flow.listen_to(socket.clone());
            let area = WINDOW.rect();
            assert!(!canvases.sling_command(Command::Dictate, &panes, area), "nothing selected");
            select_the_stroke(&mut canvases, &panes);

            dictate_and_stop(&mut canvases, &panes);
            let said = heard.lock().unwrap().clone();
            assert_eq!(said, [r#"["dictation","start"]"#, r#"["dictation","stop"]"#]);

            // Flick types what it heard into the focused entry.
            assert!(!canvases.sling_typed("what is this drawing?"));
            assert!(canvases.sling_key(Chord::new(0, '\r')));
            gone(&mut canvases);
            delivered(home.path(), "what is this drawing?");

            // Escape while Flick listens cancels the dictation.
            canvases.clear_selection(&panes, panes.focus());
            select_the_stroke(&mut canvases, &panes);
            assert!(canvases.sling_command(Command::Dictate, &panes, area));
            assert!(canvases.sling_key(Chord::new(0, '\u{1b}')));
            wait_until(|| heard.lock().unwrap().len() == 4);
            assert_eq!(heard.lock().unwrap()[3], r#"["dictation","cancel"]"#);
            assert_eq!(shown(&canvases, &panes).labels.len(), 0, "its late answer shows nothing");
            daemon.session_stop("notes").unwrap();
        });
    }

    #[test]
    fn with_flick_not_running_dictate_says_so_once_and_typing_still_slings() {
        let (bin, flick) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        with_store(Some(&fake_claude(bin.path())), |daemon| {
            let home = open_notes(daemon);
            user_stroke(daemon, "notes");
            daemon.session_start("notes").unwrap();
            let (mut canvases, panes) = (Canvases::new(daemon.clone()), notes());
            canvases.flow.listen_to(flick.path().join("flick.sock"));
            select_the_stroke(&mut canvases, &panes);

            assert!(canvases.sling_command(Command::Dictate, &panes, WINDOW.rect()));
            wait_until(|| canvases.flow.poll_dictation());
            let failed = shown(&canvases, &panes);
            assert!(failed.entry.is_some(), "the entry stays open");
            let lines = texts(&failed);
            assert_eq!(
                lines,
                ["✗ Flick is not running", "start Flick to dictate, or type the prompt"]
            );
            assert!(!canvases.flow.poll_dictation(), "said once");

            assert!(!canvases.sling_typed("typed instead"));
            assert!(canvases.sling_key(Chord::new(0, '\r')));
            gone(&mut canvases);
            delivered(home.path(), "typed instead");
            daemon.session_stop("notes").unwrap();
        });
    }
}
