//! The sling from the window: what the user is slinging
//! ([`crate::scene::sling`]) tied to the selection it sends and the ink it
//! waits for, and to the thread that sends it ([`crate::backend::Slinger`]).
//! Runs on the main thread and never waits.

use super::{Canvases, canvas_of, pane_shown};
use crate::commands::Chord;
use crate::config::Theme;
use crate::layout::{Point, Rect};
use crate::panes::Panes;
use crate::scene::Scene;
use crate::scene::select::Selection;
use crate::scene::sling::Key;

impl Canvases {
    /// The `sling` command: opens the entry at the selection, when the
    /// focused pane of `panes` laid in `area` shows it. False when it does
    /// not, or a sling is on its way.
    pub fn open_sling(&mut self, panes: &Panes, area: Rect) -> bool {
        let Some((selection, around, within)) = self.selection_in(panes, area) else {
            return false;
        };
        let selection = selection.clone();
        self.sling.open(&selection, around, within)
    }

    /// Slings the selection the focused pane of `panes` laid in `area`
    /// shows, with `prompt`, without the entry: for a prompt that comes
    /// some other way (dictation, a button on a live item). False when that
    /// pane shows no selection, or a sling is on its way.
    #[cfg_attr(not(test), expect(dead_code, reason = "dictation calls it (slinger-6c99)"))]
    pub fn sling_now(&mut self, panes: &Panes, area: Rect, prompt: &str) -> bool {
        let Some((selection, around, within)) = self.selection_in(panes, area) else {
            return false;
        };
        let selection = selection.clone();
        let sent = self.sling.send_now(&selection, prompt, around, within);
        self.poll_sling();
        sent
    }

    /// Whether the sling's entry is open: it takes the keys.
    pub fn typing(&self) -> bool {
        self.sling.is_typing()
    }

    /// Whether a key no command took is a text's: every key while the entry
    /// is open, and a key that types while a text is being typed.
    pub fn takes(&self, chord: Chord) -> bool {
        self.typing() || self.editing() && chord.types()
    }

    /// A key press while the entry is open: return sends and escape
    /// cancels (true: draw). False for any other key, which is the
    /// entry's own and goes on to it, and for every key while it is closed.
    pub fn sling_key(&mut self, chord: Chord) -> bool {
        match self.sling.key(chord) {
            Some(Key::Send) => {
                self.sling.send();
                self.poll_sling();
                true
            }
            Some(Key::Cancel) => self.sling.cancel(),
            Some(Key::Type) | None => false,
        }
    }

    /// The entry's text is now `text`. Nothing more to draw (false): the
    /// entry shows it already.
    pub fn sling_typed(&mut self, text: &str) -> bool {
        self.sling.typed(text);
        false
    }

    /// Whether `at`, a point in the window, is on the open entry: a press
    /// there is the entry's.
    pub fn sling_covers(&self, at: Point) -> bool {
        self.sling.covers(at)
    }

    /// Lays the sling over `scene` in `theme`'s colours: its entry, or its
    /// line while it goes, or why it failed.
    pub fn show_sling(&self, scene: &mut Scene, theme: &Theme) {
        scene.entry = self.sling.entry().map(Box::new);
        scene.labels.extend(self.sling.labels(theme));
    }

    /// Takes the daemon's answer to the sling, and sends a waiting one once
    /// the ink of its canvas is in the daemon. Returns whether anything
    /// shown changed.
    pub(super) fn poll_sling(&mut self) -> bool {
        let mut changed = false;
        for answer in self.slinger.answers() {
            changed |= self.sling.answered(answer);
        }
        let (ink, sling) = (&self.ink, &mut self.sling);
        // A lost daemon would hold the ink, and the sling, for ever: the
        // sling goes and says so.
        if let Some(ask) = sling.ready(|c| ink.settled(c) || ink.problem().is_some()) {
            self.slinger.send(ask);
        }
        changed
    }

    /// The selection, when the focused pane of `panes` laid in `area` shows
    /// it, with its box and the pane's frame, in the window.
    fn selection_in(&self, panes: &Panes, area: Rect) -> Option<(&Selection, Rect, Rect)> {
        let (selection, pane) = (self.selection()?, panes.focus());
        let placed = panes.place(area).into_iter().find(|(id, _, _)| *id == pane);
        let frame = placed.filter(|_| canvas_of(panes, pane) == Some(&selection.canvas))?.2;
        let bounds = self.select.bounds(&pane_shown(&self.open, &self.ink, panes, pane))?;
        let view = self.views.iter().find(|(p, _)| *p == pane);
        let on = view.map(|(_, v)| v.viewport).unwrap_or_default().rect_to_screen(bounds);
        Some((selection, Rect { x: on.x + frame.x, y: on.y + frame.y, ..on }, frame))
    }
}

#[cfg(test)]
mod tests {
    use slinger_control::Client;

    use super::*;
    use crate::backend::testing::{fake_claude, user_stroke, wait_until};
    use crate::backend::tests::{open_notes, with_store};
    use crate::config::{Config, Rgb};
    use crate::layout::WINDOW;
    use crate::panes::{Kind, Pane};
    use crate::scene::ink::Pen;
    use crate::scene::tools::Tool;

    fn notes() -> Panes {
        Panes::new(Pane { kind: Kind::Canvas, name: "notes".into() })
    }

    /// What the window shows of the sling: its entry and its lines.
    fn shown(canvases: &Canvases, panes: &Panes) -> Scene {
        let theme = Config::default().theme;
        let side = crate::scene::sidebar::Sidebar::new(false);
        let (uis, notes) = (crate::scene::Uis::new(), crate::scene::Notes::new());
        let mut scene = crate::scene::build(panes, &theme, &[], &side, Vec::new(), &uis, &notes);
        scene.labels.clear();
        canvases.show_sling(&mut scene, &theme);
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
        assert!(!canvases.sling_now(&panes, area, "hi"));
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
        let pen = |x, y| Pen { at: Point { x, y }, when: 0.0, touch: false };
        let lasso = (Tool::Lasso, Rgb { r: 0, g: 0, b: 0 });
        assert!(canvases.pen_down(panes, (panes.focus(), area), pen(50.0, 50.0), lasso));
        for (x, y) in [(200.0, 50.0), (200.0, 200.0), (50.0, 200.0)] {
            canvases.pen_drag(panes, pen(x, y));
        }
        canvases.pen_up(panes, pen(50.0, 50.0), Some((panes.focus(), area)));
        assert!(canvases.selection().is_some());
    }

    /// What the agent's process read, once it read anything.
    fn delivered(home: &std::path::Path) -> String {
        let mut message = String::new();
        wait_until(|| {
            message = std::fs::read_to_string(home.join("in.txt")).unwrap_or_default();
            !message.is_empty()
        });
        message
    }

    /// Opens the entry at the selection, types `prompt` and sends it.
    fn sling(canvases: &mut Canvases, panes: &Panes, prompt: &str) {
        assert!(canvases.open_sling(panes, WINDOW.rect()));
        let entry = shown(canvases, panes).entry.unwrap();
        assert!(entry.frame.y > 110.0 && entry.frame.x <= 100.0, "under the stroke: {entry:?}");
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
            wait_until(|| canvases.poll() && !canvases.typing());
            let went = shown(&canvases, &panes);
            assert_eq!((went.entry, went.labels.len()), (None, 0), "it went: nothing more shows");
            let message = delivered(home.path());
            assert!(message.contains("selected=c1") && message.contains("what is this?"));

            assert!(canvases.open_sling(&panes, WINDOW.rect()));
            assert!(canvases.sling_key(Chord::new(0, '\u{1b}')), "escape cancels");
            assert!(!canvases.typing());
            assert!(canvases.sling_now(&panes, WINDOW.rect(), "and now?"));
            wait_until(|| canvases.poll());
            daemon.session_stop("notes").unwrap();
        });
    }
}
