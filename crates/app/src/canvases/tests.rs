//! The first gesture after a text being typed ends reaches the daemon, for
//! every tool (slinger-a77a12).

use super::*;
use crate::backend::testing::{user_stroke, wait_until};
use crate::backend::tests::{open_notes, with_store};
use crate::config::Config;
use crate::layout::WINDOW;
use crate::panes::Pane;
use crate::scene::ink::Pen;

const GREY: Rgb = Rgb { r: 128, g: 128, b: 128 };

fn notes() -> Panes {
    Panes::new(Pane { kind: Kind::Canvas, name: "notes".into() })
}

fn pen(x: f64, y: f64, when: f64) -> Pen {
    Pen { at: Point { x, y }, when, touch: false }
}

/// The kinds of the items the daemon holds on the first page of `notes`.
fn held(daemon: &Client) -> Vec<String> {
    let items = daemon.canvas_show("notes", None).unwrap().items;
    items.iter().map(|i| i["kind"].as_str().unwrap_or_default().to_string()).collect()
}

fn count(daemon: &Client, kind: &str) -> usize {
    held(daemon).iter().filter(|k| *k == kind).count()
}

/// Takes in what the daemon said and lays the pane out, as each event does.
fn shown(canvases: &mut Canvases, panes: &Panes) -> usize {
    canvases.poll();
    canvases.show(panes, WINDOW.rect(), &Config::default().theme).len()
}

/// A tap with the text tool at `at`, then "word" typed: left open.
fn type_word(canvases: &mut Canvases, panes: &Panes, at: (f64, f64), when: f64) {
    let pane = (panes.focus(), WINDOW.rect());
    assert!(canvases.pen_down(panes, pane, pen(at.0, at.1, when), (Tool::Text, GREY)));
    canvases.pen_up(panes, pen(at.0, at.1, when + 0.1), Some(pane));
    assert!(canvases.typed("w") && canvases.typed("o") && canvases.typed("r"));
    assert!(canvases.typed("d") && canvases.editing());
    shown(canvases, panes);
}

/// A press with `tool` at the first point, a drag through the others and a
/// lift at the last: each used, none lost.
fn gesture(canvases: &mut Canvases, panes: &Panes, tool: Tool, path: &[(f64, f64)], when: f64) {
    let pane = (panes.focus(), WINDOW.rect());
    let (first, last) = (path[0], path[path.len() - 1]);
    assert!(canvases.pen_down(panes, pane, pen(first.0, first.1, when), (tool, GREY)), "{tool:?}");
    for (i, &(x, y)) in path.iter().enumerate().skip(1) {
        let at = when + 0.01 * f64::from(u32::try_from(i).unwrap());
        assert!(canvases.pen_drag(panes, pen(x, y, at)), "{tool:?}: the drag is the tool's");
    }
    assert!(canvases.pen_up(panes, pen(last.0, last.1, when + 0.5), Some(pane)), "{tool:?}");
    shown(canvases, panes);
}

/// What the one pane's footer says, part by part, with `tool`.
fn footer(canvases: &Canvases, panes: &Panes, tool: Tool) -> Vec<String> {
    let (theme, keys) = (Config::default().theme, crate::commands::Keymap::default());
    let footers = canvases.footers(panes, WINDOW.rect(), &theme, (tool, GREY), &keys);
    let part = |p: &crate::scene::status::Part| p.runs.iter().map(|r| r.text.as_str()).collect();
    footers[0].parts.iter().map(part).collect()
}

/// Waits until the daemon holds what `met` asks, the pane taking in its news.
fn until(canvases: &mut Canvases, panes: &Panes, daemon: &Client, met: impl Fn(&Client) -> bool) {
    wait_until(|| {
        shown(canvases, panes);
        met(daemon)
    });
}

#[test]
fn escape_ends_a_text_and_the_next_box_drag_is_a_box() {
    with_store(None, |daemon| {
        let _home = open_notes(daemon);
        let (mut canvases, panes) = (Canvases::new(daemon.clone()), notes());
        wait_until(|| shown(&mut canvases, &panes) == 1);
        type_word(&mut canvases, &panes, (300.0, 300.0), 10.0);
        assert!(canvases.typed("\u{1b}") && !canvases.editing());
        until(&mut canvases, &panes, daemon, |d| count(d, "text") == 1);
        gesture(
            &mut canvases,
            &panes,
            Tool::Box,
            &[(400.0, 400.0), (450.0, 440.0), (500.0, 480.0)],
            14.0,
        );
        until(&mut canvases, &panes, daemon, |d| count(d, "box") == 1);
    });
}

#[test]
fn the_first_press_of_any_tool_after_a_text_left_open_ends_it_and_is_that_tool_s() {
    with_store(None, |daemon| {
        let _home = open_notes(daemon);
        user_stroke(daemon, "notes");
        let (mut canvases, panes) = (Canvases::new(daemon.clone()), notes());
        wait_until(|| shown(&mut canvases, &panes) == 1);
        let mut texts = 0;
        let mut word = |canvases: &mut Canvases, when: f64| {
            type_word(canvases, &panes, (600.0, 100.0 + 80.0 * f64::from(texts)), when);
            texts += 1;
            texts
        };

        let n = word(&mut canvases, 10.0);
        let said = footer(&canvases, &panes, Tool::Text);
        assert!(said[0].starts_with("Text") && said[0].ends_with(" · typing"), "{said:?}");
        assert!(said[1].ends_with(" · 1/1"), "the page: {said:?}");
        gesture(&mut canvases, &panes, Tool::Box, &[(400.0, 400.0), (500.0, 480.0)], 12.0);
        until(&mut canvases, &panes, daemon, |d| count(d, "text") == 1 && count(d, "box") == 1);
        assert!(!canvases.editing() && n == 1);

        word(&mut canvases, 20.0);
        gesture(&mut canvases, &panes, Tool::Arrow, &[(400.0, 520.0), (500.0, 560.0)], 22.0);
        until(&mut canvases, &panes, daemon, |d| count(d, "text") == 2 && count(d, "arrow") == 1);

        // The lasso's loop around the user's stroke, from (100, 100) to (110, 110).
        word(&mut canvases, 30.0);
        let around = [(80.0, 80.0), (130.0, 80.0), (130.0, 130.0), (80.0, 130.0), (80.0, 81.0)];
        gesture(&mut canvases, &panes, Tool::Lasso, &around, 32.0);
        assert_eq!(canvases.selection().map(|s| s.ids.len()), Some(1), "the stroke is selected");
        assert!(footer(&canvases, &panes, Tool::Lasso)[0].ends_with(" · 1 selected"));
        until(&mut canvases, &panes, daemon, |d| count(d, "text") == 3);
        assert!(canvases.clear_selection(&panes, panes.focus()));

        word(&mut canvases, 40.0);
        gesture(&mut canvases, &panes, Tool::Eraser, &[(90.0, 90.0), (120.0, 120.0)], 42.0);
        until(&mut canvases, &panes, daemon, |d| count(d, "text") == 4 && count(d, "stroke") == 0);

        word(&mut canvases, 50.0);
        gesture(&mut canvases, &panes, Tool::Pen, &[(700.0, 500.0), (720.0, 520.0)], 52.0);
        until(&mut canvases, &panes, daemon, |d| count(d, "text") == 5 && count(d, "stroke") == 1);
    });
}
