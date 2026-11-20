//! A context click pops the menu for what it was on (slinger-d1532a): the
//! empty canvas, a pane's footer, an agent UI pane's footer; the click
//! focuses its pane and moves the pointer first, so a pick acts there.

use super::*;
use crate::commands::context::{Popup, Subject, items};
use crate::commands::menu::Action;
use crate::scene::secondary::{Button, Phase};
use crate::scene::status;

impl World {
    /// A right press and lift at `x`, `y`: the menu the window would pop.
    pub(super) fn context(&mut self, x: f64, y: f64) -> Option<Popup> {
        let at = Point { x, y };
        let down = Event::Button { button: Button::Secondary, phase: Phase::Down, at };
        self.send(down);
        let up = Event::Button { button: Button::Secondary, phase: Phase::Up, at };
        let w = self;
        let routed = route(
            up,
            &mut w.shared,
            &mut w.canvases,
            &mut w.tools,
            &w.settings,
            &w.socket,
            &mut w.sidebar,
        );
        match routed {
            Routed::Answer(Step::Menu(popup, _)) => Some(popup),
            _ => None,
        }
    }

    /// The menu `subject` offers now, by the window's facts.
    pub(super) fn offers(&self, subject: &Subject) -> Vec<crate::commands::menu::Item> {
        let area = self.sidebar.panes(self.shared.size.rect());
        let facts =
            crate::window::facts(&self.shared, &self.canvases, &self.tools, &self.sidebar, area);
        let checked = status::command(self.tools.chosen());
        items(subject, &self.settings.config.keys, checked, &facts)
    }

    /// A point on the footer of the `n`th pane, laid left to right.
    pub(super) fn footer(&self, n: usize) -> Point {
        let area = self.sidebar.panes(self.shared.size.rect());
        let frame = self.shared.panes.place(area)[n].2;
        let content = status::content(frame);
        Point { x: frame.x + frame.width / 2.0, y: content.y + content.height + 4.0 }
    }
}

pub(super) fn titles(popup: &Popup) -> Vec<&'static str> {
    popup.items.iter().filter(|i| i.action != Action::Separator).map(|i| i.title).collect()
}

/// Each text on the daemon's `notes` canvas, with its `x` and `y`.
fn texts(daemon: &control::Client) -> Vec<(String, f64, f64)> {
    let items = daemon.canvas_show("notes", None).unwrap().items;
    let at = |i: &serde_json::Value, k: &str| i[k].as_f64().unwrap();
    let texts = items.iter().filter(|i| i["kind"] == "text");
    texts.map(|i| (i["text"].as_str().unwrap().to_string(), at(i, "x"), at(i, "y"))).collect()
}

#[test]
fn a_click_on_the_empty_canvas_pops_the_canvas_menu_at_that_point() {
    with_store(None, |daemon| {
        let _home = open_notes(daemon);
        let config = tempfile::tempdir().unwrap();
        let mut w = world(daemon, config.path());
        let popup = w.context(300.0, 250.0).expect("a menu");
        assert_eq!(popup.at, Point { x: 300.0, y: 250.0 });
        assert_eq!(popup.items, w.offers(&Subject::Canvas));
        assert_eq!(titles(&popup)[..3], ["Paste", "New text", "Select all"]);
        let step_out = popup.items.iter().find(|i| i.title == "Step out of box").unwrap();
        assert!(!step_out.enabled, "not stepped into a box");
    });
}

#[test]
fn a_click_on_a_footer_pops_the_pane_menu_and_its_pick_splits_that_pane() {
    with_store(None, |daemon| {
        let _home = open_notes(daemon);
        let config = tempfile::tempdir().unwrap();
        let mut w = world(daemon, config.path());
        let close = |p: &Popup| p.items.iter().find(|i| i.title == "Close pane").unwrap().enabled;
        let alone = w.context(w.footer(0).x, w.footer(0).y).expect("a menu");
        assert!(!close(&alone), "one pane: Close pane is grey");
        let left = w.shared.panes.focus();
        assert!(w.send(Event::Command(Command::SplitRight)));
        let right = w.shared.panes.focus();
        let at = w.footer(0);
        let popup = w.context(at.x, at.y).expect("a menu");
        assert_eq!(w.shared.panes.focus(), left, "the click focused its pane");
        assert_eq!(titles(&popup), ["Split right", "Split down", "Close pane"]);
        assert!(close(&popup), "two panes");

        assert!(w.send(Event::Command(Command::SplitRight)), "the pick");
        let area = w.sidebar.panes(w.shared.size.rect());
        let placed: Vec<_> = w.shared.panes.place(area).into_iter().map(|(id, ..)| id).collect();
        let new = w.shared.panes.focus();
        assert_eq!(placed, [left, new, right], "split beside the clicked pane");
    });
}

#[test]
fn an_agent_ui_pane_s_footer_pops_its_menu_and_its_page_keeps_its_own() {
    with_store(None, |daemon| {
        let _home = open_notes(daemon);
        let config = tempfile::tempdir().unwrap();
        let mut w = world(daemon, config.path());
        w.shared.panes = Panes::new(Pane { kind: PaneKind::Ui, name: "board".into() });
        let at = w.footer(0);
        let popup = w.context(at.x, at.y).expect("a menu");
        let all =
            ["Reload agent UI", "Export agent UI…", "Split right", "Split down", "Close pane"];
        assert_eq!(titles(&popup), all);
        assert_eq!(popup.items, w.offers(&Subject::Pane { ui: true }));
        let on_page = Event::Button {
            button: Button::Secondary,
            phase: Phase::Down,
            at: Point { x: 300.0, y: 200.0 },
        };
        assert!(!w.send(on_page), "a press on the page goes on to it");
        assert!(w.context(300.0, 200.0).is_none());
    });
}

#[test]
fn under_the_open_palette_a_click_pops_nothing() {
    with_store(None, |daemon| {
        let _home = open_notes(daemon);
        let config = tempfile::tempdir().unwrap();
        let mut w = world(daemon, config.path());
        assert!(w.send(Event::Command(Command::ShowCommands)));
        assert!(w.shared.palette.is_open());
        assert!(w.context(300.0, 250.0).is_none());
        assert!(w.shared.palette.is_open(), "still open");
    });
}

#[test]
fn new_text_picked_after_a_click_starts_a_text_at_the_clicked_point() {
    with_store(None, |daemon| {
        let _home = open_notes(daemon);
        let config = tempfile::tempdir().unwrap();
        let mut w = world(daemon, config.path());
        for (x, y, word) in [(200.0, 150.0, "a"), (500.0, 300.0, "b")] {
            assert!(w.context(x, y).is_some());
            assert!(w.send(Event::Command(Command::NewText)), "the pick");
            assert!(w.canvases.editing());
            w.key(Chord::new(0, word.chars().next().unwrap()), word);
            w.key(Chord::new(0, '\u{1b}'), "\u{1b}");
        }
        w.until(|_| texts(daemon).len() == 2);
        let found = texts(daemon);
        let (a, b) = (&found[0], &found[1]);
        // The file keeps tenths of a point: 300 and 150 points apart, at 100%.
        let apart = (b.1 - a.1, b.2 - a.2);
        assert!((apart.0 - 3000.0).abs() < 10.0 && (apart.1 - 1500.0).abs() < 10.0, "{found:?}");
    });
}
