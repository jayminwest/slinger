//! The first gesture after a text being typed ends reaches the daemon, for
//! every tool (slinger-a77a12).

use super::*;
use crate::backend::testing::{user_stroke, wait_until};
use crate::backend::tests::{open_notes, with_store};
use crate::config::Config;
use crate::layout::{Point, WINDOW};
use crate::panes::Pane;
use crate::scene::ink::Pen;

const GREY: Rgb = Rgb { r: 128, g: 128, b: 128 };

fn notes() -> Panes {
    Panes::new(Pane { kind: Kind::Canvas, name: "notes".into() })
}

fn pen(x: f64, y: f64, when: f64) -> Pen {
    Pen { at: Point { x, y }, when, touch: false, eraser: false, shift: false }
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
        assert!(
            canvases.pen_drag(panes, pen(x, y, at), Some(pane)),
            "{tool:?}: the drag is the tool's"
        );
    }
    assert!(canvases.pen_up(panes, pen(last.0, last.1, when + 0.5), Some(pane)), "{tool:?}");
    shown(canvases, panes);
}

/// What the one pane's footer says, its texts joined by "|", with `tool`.
fn footer(canvases: &mut Canvases, panes: &Panes, tool: Tool) -> String {
    let (theme, keys) = (Config::default().theme, crate::commands::Keymap::default());
    let footers = canvases.footers(panes, WINDOW.rect(), &theme, (tool, GREY), &keys, false);
    let part = |p: &crate::scene::status::Part| p.runs.iter().map(|r| r.text.as_str()).collect();
    footers[0].parts.iter().map(part).collect::<Vec<String>>().join("|")
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
        let said = footer(&mut canvases, &panes, Tool::Text);
        assert!(said.starts_with("Text") && said.contains("|· typing|"), "{said:?}");
        assert!(said.contains(" · 1/1|"), "the page: {said:?}");
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
        assert!(footer(&mut canvases, &panes, Tool::Lasso).contains("|· 1 selected|"));
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

#[test]
fn a_paste_types_into_the_text_and_each_page_command_says_what_it_did() {
    use crate::commands::Command;
    with_store(None, |daemon| {
        let _home = open_notes(daemon);
        let (mut canvases, panes) = (Canvases::new(daemon.clone()), notes());
        wait_until(|| shown(&mut canvases, &panes) == 1);
        assert!(!canvases.pasted("no text is open"), "nothing takes it");
        type_word(&mut canvases, &panes, (300.0, 300.0), 10.0);
        assert!(canvases.pasted(" and\r\nmore"));
        assert!(canvases.typed("\u{1b}"));
        let pasted = |d: &Client| {
            let items = d.canvas_show("notes", None).unwrap().items;
            items.iter().any(|i| i["text"] == "word and\nmore")
        };
        until(&mut canvases, &panes, daemon, pasted);

        let pane = (panes.focus(), WINDOW.rect());
        assert_eq!(canvases.said(), None);
        assert!(canvases.command(&panes, pane, Command::RemovePage));
        assert_eq!(canvases.said().as_deref(), Some("The last page stays"));
        assert!(canvases.command(&panes, pane, Command::NewPage));
        assert_eq!(canvases.said().as_deref(), Some("New page: Page 2 · 2/2"));
        wait_until(|| {
            shown(&mut canvases, &panes);
            footer(&mut canvases, &panes, Tool::Pen).contains("|Page 2 · 2/2|")
        });
        assert!(canvases.command(&panes, pane, Command::NextPage));
        assert_eq!(canvases.said().as_deref(), Some("Page 1 · 1/2"));
        assert_eq!(canvases.said(), None, "said once");
        // The page menu lists the pages, and a tap on one turns the pane to it.
        let names = Some((vec!["Page 1".to_string(), "Page 2".to_string()], 0));
        wait_until(|| {
            shown(&mut canvases, &panes);
            canvases.page_names(&panes, panes.focus()) == names
        });
        canvases.turn_to(&panes, panes.focus(), 1);
        assert_eq!(canvases.said().as_deref(), Some("Page 2 · 2/2"));
        canvases.turn_to(&panes, panes.focus(), 9);
        assert_eq!(canvases.said(), None, "no such page: nothing turns");
        wait_until(|| {
            shown(&mut canvases, &panes);
            canvases.page_names(&panes, panes.focus()).is_some_and(|(_, at)| at == 1)
        });
    });
}

/// slinger-5578a4: a zoom key, a pinch and a scroll with cmd held all zoom the
/// pane's canvas, the scene draws at the new zoom, and each zoom flashes it.
#[test]
fn a_zoom_reaches_the_scene_and_flashes_its_level() {
    use crate::commands::Command;
    with_store(None, |daemon| {
        let _home = open_notes(daemon);
        let (mut canvases, panes) = (Canvases::new(daemon.clone()), notes());
        wait_until(|| shown(&mut canvases, &panes) == 1);
        let zoom = |canvases: &mut Canvases| {
            let theme = Config::default().theme;
            canvases.show(&panes, WINDOW.rect(), &theme)[0].scene.viewport.zoom()
        };
        let pane = (panes.focus(), WINDOW.rect());
        assert!(canvases.command(&panes, pane, Command::ZoomIn));
        assert_eq!((zoom(&mut canvases), canvases.said().as_deref()), (1.25, Some("Zoom 125%")));
        canvases.zoom(panes.focus(), Point { x: 10.0, y: 10.0 }, 2.0);
        assert_eq!((zoom(&mut canvases), canvases.said().as_deref()), (2.5, Some("Zoom 250%")));
        assert!(footer(&mut canvases, &panes, Tool::Pen).contains("|250%|"), "the footer says it");
        assert!(canvases.command(&panes, pane, Command::ZoomReset));
        assert_eq!((zoom(&mut canvases), canvases.said().as_deref()), (1.0, Some("Zoom 100%")));
        assert_eq!(canvases.said(), None, "said once");

        let ui = Panes::new(Pane { kind: Kind::Ui, name: "board".into() });
        assert!(canvases.command(&ui, (ui.focus(), WINDOW.rect()), Command::ZoomIn));
        assert_eq!(canvases.said().as_deref(), Some("No canvas in this pane to zoom"));
    });
}

/// slinger-fe0e9d: `[canvas] background` sets the dotted grid at start, and
/// `toggle-grid` flips it on the pane's scene and flashes what it did.
#[test]
fn the_grid_follows_the_config_and_the_toggle_and_flashes() {
    use crate::commands::Command;
    use crate::config::canvas::{Background, Canvas};
    with_store(None, |daemon| {
        let _home = open_notes(daemon);
        let (mut canvases, panes) = (Canvases::new(daemon.clone()), notes());
        canvases.follow_paper(Canvas { background: Background::Dots, ..Canvas::default() });
        wait_until(|| shown(&mut canvases, &panes) == 1);
        let grid = |canvases: &mut Canvases| {
            let theme = Config::default().theme;
            canvases.show(&panes, WINDOW.rect(), &theme)[0].scene.grid.map(|g| g.spacing)
        };
        assert_eq!(grid(&mut canvases), Some(28.35));
        let pane = (panes.focus(), WINDOW.rect());
        assert!(canvases.command(&panes, pane, Command::ToggleGrid));
        assert_eq!(grid(&mut canvases), None);
        assert_eq!(canvases.said().as_deref(), Some("Dotted grid off"));
        assert!(canvases.command(&panes, pane, Command::ToggleGrid));
        assert_eq!(canvases.said().as_deref(), Some("Dotted grid on"));
        assert_eq!(grid(&mut canvases), Some(28.35));
    });
}

/// slinger-174eda: a canvas pane knows its canvas's file once the daemon has
/// shown it, and `save` says where every change went.
#[test]
fn a_pane_knows_its_canvas_file_and_save_says_where_it_went() {
    use crate::commands::Command;
    use crate::scene::title;
    with_store(None, |daemon| {
        let _home = open_notes(daemon);
        let (mut canvases, panes) = (Canvases::new(daemon.clone()), notes());
        wait_until(|| shown(&mut canvases, &panes) == 1);
        let (canvas, file) = canvases.file(&panes, panes.focus()).unwrap();
        let file = file.unwrap().to_string();
        assert_eq!(canvas, "notes");
        assert!(file.ends_with("/canvases/notes.canvas"), "{file}");
        assert!(std::fs::metadata(&file).unwrap().is_file());
        assert!(canvases.command(&panes, (panes.focus(), WINDOW.rect()), Command::Save));
        let home = std::env::home_dir();
        let said = format!("Saved to {}", title::tilde(&file, home.as_deref()));
        assert_eq!(canvases.said(), Some(said));

        let ui = Panes::new(Pane { kind: Kind::Ui, name: "board".into() });
        assert_eq!(canvases.file(&ui, ui.focus()), None);
        assert!(canvases.command(&ui, (ui.focus(), WINDOW.rect()), Command::Save));
        assert_eq!(canvases.said().as_deref(), Some(title::NO_CANVAS));
    });
}
