//! The window's routing, event by event as `platform` hands them in: a text
//! ended with escape, then a tool switch by its key, then a box drag
//! (slinger-a77a12).

use super::*;
use crate::backend::ConfigFiles;
use crate::backend::testing::wait_until;
use crate::backend::tests::{open_notes, with_store};
use crate::commands::{CMD, Chord};
use crate::panes::{Kind as PaneKind, Pane};
use crate::scene::ink::Pen;
use crate::scene::tools::Tool;

/// The window's state, as `window` keeps it, with no watch.
struct World {
    settings: Settings,
    shared: Shared,
    tools: Tools,
    sidebar: Sidebar,
    canvases: Canvases,
    socket: std::path::PathBuf,
}

impl World {
    /// Hands `event` in as the window's answer does ([`route`]); false for
    /// a pass. A tick takes only what the canvases' followers heard.
    fn send(&mut self, event: Event) -> bool {
        let w = self;
        let routed = route(
            event,
            &mut w.shared,
            &mut w.canvases,
            &mut w.tools,
            &w.settings,
            &w.socket,
            &mut w.sidebar,
        );
        match routed {
            Routed::Answer(step) | Routed::Watch(_, step) => !matches!(step, Step::Pass),
            Routed::News(_) => {
                w.canvases.poll();
                view(&w.settings, &mut w.shared, &w.tools, &w.sidebar, &mut w.canvases);
                true
            }
        }
    }

    /// A key press as `platform` sends it: its chord, and when that passes,
    /// the text it types.
    fn key(&mut self, chord: Chord, text: &str) {
        if !self.send(Event::Key(chord)) && !text.is_empty() {
            assert!(self.send(Event::Text(text.into())), "{text:?} is typed");
        }
    }
}

fn at(x: f64, y: f64, when: f64) -> Pen {
    Pen { at: Point { x, y }, when, touch: false, eraser: false, shift: false }
}

fn count(daemon: &control::Client, kind: &str) -> usize {
    let items = daemon.canvas_show("notes", None).unwrap().items;
    items.iter().filter(|i| i["kind"] == kind).count()
}

/// A window onto `daemon`'s `notes` canvas, its page arrived, with the
/// default config.
fn world(daemon: &control::Client, config: &Path) -> World {
    let settings = Settings::load(ConfigFiles::at(config.join("config.toml")));
    let panes = Panes::new(Pane { kind: PaneKind::Canvas, name: "notes".into() });
    let mut w = World {
        tools: Tools::new(settings.config.palette.clone()),
        settings,
        shared: Shared { panes, ..Shared::default() },
        sidebar: Sidebar::new(false),
        canvases: Canvases::new(daemon.clone()),
        socket: "/nonexistent/slinger.sock".into(),
    };
    w.until(|w| w.canvases.page_names(&w.shared.panes, w.shared.panes.focus()).is_some());
    w
}

impl World {
    /// Ticks until `met`, as the window's timer does.
    fn until(&mut self, met: impl Fn(&World) -> bool) {
        wait_until(|| {
            self.send(Event::Tick);
            met(self)
        });
    }

    /// The text tool by its key (cmd+4), a click at `x`, then "word" typed.
    fn type_word(&mut self, x: f64, when: f64) {
        self.key(Chord::new(CMD, '4'), "4");
        assert_eq!(self.tools.tool(), Tool::Text);
        assert!(self.send(Event::Press(at(x, 300.0, when))));
        self.send(Event::Lift(at(x, 300.0, when + 0.1)));
        for c in ['w', 'o', 'r', 'd'] {
            self.key(Chord::new(0, c), &c.to_string());
        }
        assert!(self.canvases.editing());
    }

    /// The box tool by its key (cmd+5), then a drag from `x`: each event used.
    fn drag_box(&mut self, x: f64, when: f64) {
        self.key(Chord::new(CMD, '5'), "5");
        assert_eq!(self.tools.tool(), Tool::Box);
        assert!(self.send(Event::Press(at(x, 400.0, when))));
        assert!(self.send(Event::Drag(at(x + 50.0, 440.0, when + 0.1))));
        assert!(self.send(Event::Drag(at(x + 100.0, 480.0, when + 0.2))));
        assert!(self.send(Event::Lift(at(x + 100.0, 480.0, when + 0.3))));
    }
}

#[test]
fn escape_ends_a_text_and_the_first_box_drag_after_cmd_5_reaches_the_daemon() {
    with_store(None, |daemon| {
        let _home = open_notes(daemon);
        let config = tempfile::tempdir().unwrap();
        let mut w = world(daemon, config.path());
        w.type_word(300.0, 10.0);
        w.key(Chord::new(0, '\u{1b}'), "\u{1b}");
        assert!(!w.canvases.editing(), "escape ended the text");
        w.until(|_| count(daemon, "text") == 1);
        w.drag_box(400.0, 14.0);
        w.until(|_| count(daemon, "box") == 1);
    });
}

#[test]
fn a_text_left_open_ends_and_the_first_box_drag_after_cmd_5_reaches_the_daemon() {
    with_store(None, |daemon| {
        let _home = open_notes(daemon);
        let config = tempfile::tempdir().unwrap();
        let mut w = world(daemon, config.path());
        w.type_word(300.0, 10.0);
        w.drag_box(400.0, 14.0);
        assert!(!w.canvases.editing(), "the press ended the text");
        w.until(|_| count(daemon, "text") == 1 && count(daemon, "box") == 1);
    });
}
