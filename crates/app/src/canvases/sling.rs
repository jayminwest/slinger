//! The sling from the window: what the user is slinging
//! ([`crate::scene::sling`]) tied to the selection it sends and the ink it
//! waits for, and to the thread that sends it ([`crate::backend::Slinger`]).
//! Runs on the main thread and never waits.

use super::{Canvases, pane_shown};
use crate::commands::{Chord, Command, Keymap};
use crate::config::Theme;
use crate::layout::{Point, Rect, Viewport};
use crate::panes::Panes;
use crate::scene::Scene;
use crate::scene::select::Selection;
use crate::scene::sidebar::Sidebar;
use crate::scene::sling::{Key, Profile, edits};

impl Canvases {
    /// The `sling` command: opens the entry at the selection, when the
    /// focused pane of `panes` laid in `area` shows it; else beside the
    /// first button on that pane's page, to sling what is new there. False
    /// when it shows neither, a sling is on its way, or feedback is open. A
    /// page's new name's entry closes.
    pub fn open_sling(&mut self, panes: &Panes, area: Rect) -> bool {
        if self.notes.is_typing() {
            return false;
        }
        self.asks.cancel_rename();
        let selected = self.selection_in(panes, area).map(|(s, a, w)| (s.clone(), a, w));
        let Some((selection, around, within)) = selected.or_else(|| self.button_in(panes, area))
        else {
            return false;
        };
        self.sling.open(&selection, around, within)
    }

    /// Slings `selection` (no ids: what is new on its page) with `prompt`
    /// under `profile`, without the entry; its line, and a failure's entry, stand under
    /// `around` inside `within`, in the window. For a sling that needs no
    /// typing: a tap on a button. False while a sling is on its way.
    pub(super) fn sling_now(
        &mut self,
        selection: &Selection,
        prompt: (&str, Profile),
        around: Rect,
        within: Rect,
    ) -> bool {
        let sent = self.sling.send_now(selection, prompt, around, within);
        self.poll_sling();
        sent
    }

    /// Whether an entry is open (the sling's, the feedback's or a page's new
    /// name's): it takes the keys.
    pub fn typing(&self) -> bool {
        self.sling.is_typing() || self.notes.is_typing() || self.asks.renaming()
    }

    /// Closes whichever entry is open: one shows at a time.
    pub(super) fn close_entries(&mut self) {
        if self.sling.cancel() | self.notes.cancel() | self.asks.cancel_rename() {
            self.entry_closed();
        }
    }

    /// Whether a key no command took is a text's: while the entry is open,
    /// every key a text field uses ([`edits`]) and every key `keys` binds
    /// to nothing, but its `dictate` chord; and a key that types while a
    /// text is being typed. So cmd+Q still quits with the entry open.
    pub fn takes(&self, chord: Chord, keys: &Keymap) -> bool {
        let bound = keys.command(chord);
        let field = bound.is_none() || bound != Some(Command::Dictate) && edits(chord);
        self.typing() && field || self.editing() && chord.types()
    }

    /// A key press while the entry is open: return sends and escape
    /// cancels (true: draw), and either cancels a dictation going. False
    /// for any other key, which is the entry's own and goes on to it, and
    /// for every key while it is closed.
    pub fn sling_key(&mut self, chord: Chord) -> bool {
        if let Some(closed) = self.feedback_key(chord) {
            return closed;
        }
        if self.asks.renaming() {
            let answer = match Key::of(chord) {
                Key::Send => "\r",
                Key::Cancel => "\u{1b}",
                Key::Type => return false,
            };
            if let Some((canvas, op)) = self.asks.typed(answer) {
                self.change_pages(canvas, op);
            }
            self.entry_closed();
            return true;
        }
        let closed = match self.sling.key(chord) {
            Some(Key::Send) => {
                self.sling.send();
                self.poll_sling();
                true
            }
            Some(Key::Cancel) => self.sling.cancel(),
            Some(Key::Type) | None => return false,
        };
        self.entry_closed();
        closed
    }

    /// The entry's text is now `text`. Nothing more to draw (false): the
    /// entry shows it already.
    pub fn sling_typed(&mut self, text: &str) -> bool {
        if self.asks.renaming() {
            self.asks.named(text);
        } else if !self.feedback_typed(text) {
            self.sling.typed(text);
        }
        false
    }

    /// Whether `at`, a point in the window, is on the open entry: a press
    /// there is the entry's.
    pub fn sling_covers(&self, at: Point) -> bool {
        self.sling.covers(at) || self.notes.covers(at) || self.asks.covers(at)
    }

    /// Lays the sling over `scene` in `theme`'s colours: its entry, to the
    /// agent `sidebar` names for its canvas, with `keys`' dictate key; or
    /// its line while it goes, or why it failed; the feedback entry or a
    /// page's new name's in its place while that is open, and the
    /// feedback's lines; and what dictation says.
    pub fn show_sling(&self, scene: &mut Scene, theme: &Theme, keys: &Keymap, sidebar: &Sidebar) {
        let agent = self.sling.canvas().and_then(|canvas| sidebar.agent(canvas));
        let entry = self.sling.entry(theme, agent, keys.chord(Command::Dictate));
        let (note, lines) = self.notes.shown(theme);
        scene.entry = note.or_else(|| self.asks.entry(theme)).or(entry).map(Box::new);
        scene.labels.extend(self.sling.labels(theme));
        scene.labels.extend(lines);
        scene.labels.extend(self.dictation_labels(theme));
    }

    /// Takes the daemon's answers to the sling and the feedback, and what
    /// it says of the agent since, and sends a waiting sling once the ink
    /// of its canvas is in the daemon. Returns whether anything shown
    /// changed.
    pub(super) fn poll_sling(&mut self) -> bool {
        let changed = self.poll_feedback() | self.poll_slung();
        let (ink, sling) = (&self.ink, &mut self.sling);
        // A lost daemon would hold the ink, and the sling, for ever: the
        // sling goes and says so.
        if let Some(ask) = sling.ready(|c| ink.settled(c) || ink.problem().is_some()) {
            self.going(ask);
        }
        changed
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
        let bounds =
            self.select.bounds(&pane_shown(&self.open, &self.views, &self.ink, panes, pane))?;
        let view = self.views.iter().find(|(p, _)| *p == pane);
        let on = view.map_or_else(Viewport::start, |(_, v)| v.viewport).rect_to_screen(bounds);
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
    const PEN: Pen =
        Pen { at: Point { x: 0.0, y: 0.0 }, when: 0.0, touch: false, eraser: false, shift: false };

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
            !canvases.typing() && canvases.sling.labels(&theme).is_empty()
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
        let shown_key = entry.dictate.map(|row| row.label.text);
        assert_eq!(shown_key, dictate.map(|key| format!("{key} dictate")), "the live key");
        let on = Point { x: entry.frame.x + 1.0, y: entry.frame.y + 1.0 };
        assert!(canvases.sling_covers(on) && canvases.typing());
        assert!(!canvases.sling_key(Chord::new(0, 'w')), "a letter is the entry's");
        assert!(!canvases.sling_typed(prompt));
        assert!(canvases.sling_key(Chord::new(0, '\r')));
        assert_eq!(texts(&shown(canvases, panes)), [format!("slinging 1 item: {prompt}")]);
    }

    #[test]
    fn a_lasso_s_selection_slings_shows_why_it_failed_then_reaches_the_agent() {
        let bin = tempfile::tempdir().unwrap();
        with_store(Some(&fake_claude(bin.path())), |daemon| {
            let home = open_notes(daemon);
            user_stroke(daemon, "notes");
            let (mut canvases, panes) = (Canvases::new(daemon.clone()), notes());
            select_the_stroke(&mut canvases, &panes);

            sling(&mut canvases, &panes, "what is this?");
            wait_until(|| canvases.poll() && canvases.typing());
            let failed = shown(&canvases, &panes);
            assert_eq!(failed.entry.as_ref().map(|e| e.text.as_str()), Some("what is this?"));
            let lines = texts(&failed);
            assert!(lines[0].starts_with("✗ ") && lines[1].contains("session start"), "{lines:?}");

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
                &selection,
                ("and now?", Profile::Running),
                around,
                WINDOW.rect()
            ));
            assert!(
                !canvases.sling_now(
                    &selection,
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
        wait_until(|| canvases.poll_dictation());
        assert!(canvases.sling_command(Command::Dictate, panes, area), "again: stop");
        assert_eq!(texts(&shown(canvases, panes)), ["Flick is writing it down…"]);
        wait_until(|| canvases.poll_dictation());
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
            canvases.listen_to(socket.clone());
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
            canvases.listen_to(flick.path().join("flick.sock"));
            select_the_stroke(&mut canvases, &panes);

            assert!(canvases.sling_command(Command::Dictate, &panes, WINDOW.rect()));
            wait_until(|| canvases.poll_dictation());
            let failed = shown(&canvases, &panes);
            assert!(failed.entry.is_some(), "the entry stays open");
            let lines = texts(&failed);
            assert_eq!(
                lines,
                ["✗ Flick is not running", "start Flick to dictate, or type the prompt"]
            );
            assert!(!canvases.poll_dictation(), "said once");

            assert!(!canvases.sling_typed("typed instead"));
            assert!(canvases.sling_key(Chord::new(0, '\r')));
            gone(&mut canvases);
            delivered(home.path(), "typed instead");
            daemon.session_stop("notes").unwrap();
        });
    }
}
