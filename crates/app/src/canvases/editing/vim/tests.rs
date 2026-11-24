//! A real Nvim (`--clean`, its homes in a temp dir) edits a text in
//! [`Shapes`]: no way of ending loses it. Each test skips, with a line on
//! stderr, on a machine with no nvim.

use std::sync::Arc;
use std::time::{Duration, Instant};

use super::*;
use crate::backend::nvim::tests::nvim;
use crate::config::{Rgb, Theme};
use crate::layout::{Point, Rect};
use crate::nvim::grid::Grid;
use crate::scene::canvas::{Item, ItemId, Shape};
use crate::scene::ink::Change;
use crate::scene::shapes::Press;
use crate::scene::tools::Tool;

const HERE: (&str, &str) = ("main", "p1");
const RED: Rgb = Rgb { r: 255, g: 0, b: 0 };

/// A Vim with an Nvim to start, `ready_within` to be ready, and its temp dir.
fn vim(ready_within: Duration) -> Option<(Vim, tempfile::TempDir)> {
    let program = nvim()?;
    let dir = tempfile::tempdir().unwrap();
    let mut vim = Vim::new(&Wake::none());
    vim.set(Setup::clean_in(program, dir.path()), ready_within);
    Some((vim, dir))
}

/// Shapes typing a new text.
fn new_text(ink: &mut Ink) -> Shapes {
    let mut shapes = Shapes::default();
    shapes.start(HERE, Point { x: 0.0, y: 0.0 }, RED, ink);
    shapes
}

/// Shapes editing a text, item i7, that reads `text`.
fn opened(text: &str, ink: &mut Ink) -> Shapes {
    let frame = Rect { x: 0.0, y: 0.0, width: 200.0, height: 30.0 };
    let shape = Shape::Text { frame, text: text.into(), size: 24.0, column: None };
    let item = Item::plain(ItemId(7), RED, shape);
    let mut shapes = Shapes::default();
    shapes.start_at(HERE, (Point { x: 5.0, y: 5.0 }, 10.0), &[Cow::Owned(item)], RED, ink);
    shapes
}

/// Polls until `done`, at most 10 s.
fn until(vim: &mut Vim, shapes: &mut Shapes, ink: &mut Ink, done: impl Fn(&Vim, &Shapes) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !done(vim, shapes) {
        if Instant::now() >= deadline {
            let g = grid(shapes).map(|(g, _)| {
                (g.cols, g.rows, (0..g.rows).map(|r| g.row_text(r)).collect::<Vec<_>>())
            });
            panic!("nvim did not get there in 10 s: {g:?}");
        }
        vim.poll(shapes, ink);
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// The text the change ending a text writes, if any.
fn written(ink: &mut Ink) -> Option<String> {
    match ink.next()?.1 {
        Change::Put { shape: Shape::Text { text, .. }, .. }
        | Change::Set { shape: Shape::Text { text, .. }, .. } => Some(text),
        other => panic!("not a text: {other:?}"),
    }
}

use std::borrow::Cow;

#[test]
fn keys_typed_before_nvim_is_ready_land_in_order_and_escape_twice_ends_the_text() {
    let Some((mut vim, _dir)) = vim(READY) else { return };
    let mut ink = Ink::named("t");
    let mut shapes = new_text(&mut ink);
    assert!(vim.typed("hello", &shapes), "an Nvim takes the keys from the start");
    assert!(vim.editing());
    assert!(vim.typed(" world", &shapes));
    until(&mut vim, &mut shapes, &mut ink, |_, s| {
        s.typing_text().as_deref() == Some("hello world")
    });
    let esc = Chord::new(0, '\u{1b}');
    assert_eq!(vim.key(esc, &mut shapes, &mut ink), Some(true), "insert mode: escape is Nvim's");
    assert!(shapes.editing());
    assert_eq!(
        vim.key(esc, &mut shapes, &mut ink),
        Some(true),
        "normal mode: escape ends the text"
    );
    assert!(!shapes.editing() && !vim.editing());
    assert_eq!(written(&mut ink).as_deref(), Some("hello world"));
}

#[test]
fn a_text_left_before_its_buffer_is_confirmed_keeps_what_it_had() {
    let Some((mut vim, _dir)) = vim(READY) else { return };
    let mut ink = Ink::named("t");
    let mut shapes = opened("plan", &mut ink);
    assert!(vim.typed("ddiGONE", &shapes));
    assert!(!vim.confirmed());
    assert!(shapes.end(&mut ink));
    vim.sync(&shapes);
    assert!(!vim.editing(), "the text ended: its Nvim is let go");
    assert_eq!(written(&mut ink), None, "an edit not confirmed changes nothing");
}

#[test]
fn a_crash_ends_the_text_with_the_buffer_last_confirmed() {
    let Some((mut vim, _dir)) = vim(READY) else { return };
    let mut ink = Ink::named("t");
    let mut shapes = new_text(&mut ink);
    vim.typed("kept", &shapes);
    until(&mut vim, &mut shapes, &mut ink, |_, s| s.typing_text().as_deref() == Some("kept"));
    vim.crash();
    until(&mut vim, &mut shapes, &mut ink, |_, s| !s.editing());
    assert_eq!(written(&mut ink).as_deref(), Some("kept"));
    assert_eq!(vim.take_flash(), None, "a text it ended needs no word");
}

#[test]
fn quitting_nvim_ends_the_text_with_its_buffer() {
    let Some((mut vim, _dir)) = vim(READY) else { return };
    let mut ink = Ink::named("t");
    let mut shapes = opened("plan", &mut ink);
    vim.typed("A!", &shapes);
    vim.key(Chord::new(0, '\u{1b}'), &mut shapes, &mut ink);
    vim.typed(":q", &shapes);
    vim.key(Chord::new(0, '\r'), &mut shapes, &mut ink);
    until(&mut vim, &mut shapes, &mut ink, |_, s| !s.editing());
    assert_eq!(written(&mut ink).as_deref(), Some("plan!"));
}

#[test]
fn an_nvim_that_cannot_start_leaves_the_text_to_the_apps_editor_and_says_why() {
    let dir = tempfile::tempdir().unwrap();
    let mut vim = Vim::new(&Wake::none());
    vim.set(Setup::clean_in(dir.path().join("no-nvim"), dir.path()), READY);
    let mut ink = Ink::named("t");
    let mut shapes = opened("plan", &mut ink);
    assert!(!vim.typed("x", &shapes), "the app's editor types");
    assert!(shapes.editing());
    let flash = vim.take_flash().unwrap();
    assert!(
        flash.starts_with("nvim did not start") && flash.ends_with("([editor] nvim)"),
        "{flash}"
    );
    assert_eq!(vim.take_flash(), None, "once");
    shapes.end(&mut ink);
    let mut ink2 = Ink::named("t2");
    let shapes = new_text(&mut ink2);
    assert!(!vim.typed("x", &shapes), "and no Nvim starts again this run");
}

#[test]
fn an_nvim_not_ready_in_time_leaves_the_text_as_it_was() {
    let Some((mut vim, _dir)) = vim(Duration::ZERO) else { return };
    let mut ink = Ink::named("t");
    let mut shapes = opened("plan", &mut ink);
    vim.typed("ddiGONE", &shapes);
    std::thread::sleep(Duration::from_millis(5));
    vim.poll(&mut shapes, &mut ink);
    assert!(!vim.editing());
    assert_eq!(shapes.typing_text().as_deref(), Some("plan"));
    let flash = vim.take_flash().unwrap();
    assert!(flash.starts_with("nvim was not ready in 0 s"), "{flash}");
}

/// The grid the text being typed shows, if one does, and its frame.
fn grid(shapes: &Shapes) -> Option<(Arc<Grid>, Rect)> {
    let shown = shapes.shown(HERE, Vec::new(), 1.0, &Theme::default());
    shown.iter().find_map(|item| match &item.shape {
        Shape::Cells { grid, frame, .. } => Some((Arc::clone(grid), *frame)),
        _ => None,
    })
}

#[test]
fn the_grid_is_80_cells_and_as_tall_as_the_text_and_a_gutter_widens_it() {
    let Some((mut vim, _dir)) = vim(READY) else { return };
    let mut ink = Ink::named("t");
    let mut shapes = opened("plan", &mut ink);
    vim.typed("A ahead", &shapes);
    let shows = |rows: usize, first: String| {
        move |_: &Vim, s: &Shapes| {
            grid(s).is_some_and(|(g, _)| g.cols == 80 && g.rows == rows && g.row_text(0) == first)
        }
    };
    until(&mut vim, &mut shapes, &mut ink, shows(2, "plan ahead".into()));
    let (_, frame) = grid(&shapes).unwrap();
    assert_eq!((frame.x, frame.y), (0.0, 0.0), "the text's top left");
    assert_eq!((frame.width, frame.height), (80.0 * 12.0, 2.0 * 24.0), "a cell half a size wide");
    vim.typed(&" word".repeat(20), &shapes);
    until(&mut vim, &mut shapes, &mut ink, shows(3, format!("plan ahead{}", " word".repeat(13))));
    vim.key(Chord::new(0, '\u{1b}'), &mut shapes, &mut ink);
    vim.typed(":set number", &shapes);
    vim.key(Chord::new(0, '\r'), &mut shapes, &mut ink);
    until(&mut vim, &mut shapes, &mut ink, |_, s| {
        grid(s).is_some_and(|(g, _)| g.cols == 84 && g.row_text(0).starts_with("  1 plan"))
    });
}

#[test]
fn a_paste_taller_than_the_grid_shows_the_text_from_its_top() {
    let Some((mut vim, _dir)) = vim(READY) else { return };
    let mut ink = Ink::named("t");
    let mut shapes = opened("plan", &mut ink);
    let lines: Vec<String> = (1..=30).map(|n| format!("line {n}")).collect();
    vim.pasted(&format!("\n{}", lines.join("\n")), &shapes);
    until(&mut vim, &mut shapes, &mut ink, |_, s| {
        grid(s).is_some_and(|(g, _)| g.rows == 32 && g.row_text(0) == "plan")
    });
}

/// A Vim whose Nvim's clipboard is the file it returns too.
fn clipped() -> Option<(Vim, tempfile::TempDir, std::path::PathBuf)> {
    let program = nvim()?;
    let dir = tempfile::tempdir().unwrap();
    let clip = dir.path().join("clip");
    let mut vim = Vim::new(&Wake::none());
    vim.set(Setup::clean_in(program, dir.path()).clip(clip.clone()), READY);
    Some((vim, dir, clip))
}

/// What Nvim's clipboard file holds, polling until `done` says so, at most 10 s.
fn clip_holds(
    vim: &mut Vim,
    shapes: &mut Shapes,
    ink: &mut Ink,
    clip: &std::path::Path,
    want: &str,
) {
    let clip = clip.to_path_buf();
    let want = want.to_string();
    until(vim, shapes, ink, move |_, _| std::fs::read_to_string(&clip).is_ok_and(|c| c == want));
}

const CMD_A: Chord = Chord::new(crate::commands::CMD, 'a');

#[test]
fn undo_and_redo_are_nvims_while_it_edits() {
    let Some((mut vim, _dir)) = vim(READY) else { return };
    let mut ink = Ink::named("t");
    let mut shapes = opened("plan", &mut ink);
    vim.typed("A ahead", &shapes);
    vim.key(Chord::new(0, '\u{1b}'), &mut shapes, &mut ink);
    until(&mut vim, &mut shapes, &mut ink, |_, s| s.typing_text().as_deref() == Some("plan ahead"));
    assert!(vim.undo(false, &shapes));
    until(&mut vim, &mut shapes, &mut ink, |_, s| s.typing_text().as_deref() == Some("plan"));
    assert!(vim.undo(true, &shapes));
    until(&mut vim, &mut shapes, &mut ink, |_, s| s.typing_text().as_deref() == Some("plan ahead"));
    shapes.end(&mut ink);
    vim.sync(&shapes);
    assert!(!vim.undo(false, &shapes), "the text ended: undo is the canvas's");
    assert_eq!(written(&mut ink).as_deref(), Some("plan ahead"), "one change for the whole edit");
}

#[test]
fn select_all_copy_and_cut_go_through_nvims_clipboard() {
    let Some((mut vim, _dir, clip)) = clipped() else { return };
    let mut ink = Ink::named("t");
    let mut shapes = opened("plan\nahead", &mut ink);
    vim.sync(&shapes);
    assert_eq!(vim.selects(), Some(false));
    assert_eq!(vim.key(CMD_A, &mut shapes, &mut ink), Some(true));
    until(&mut vim, &mut shapes, &mut ink, |v, _| v.selects() == Some(true));
    assert!(vim.copy(false, &shapes));
    clip_holds(&mut vim, &mut shapes, &mut ink, &clip, "plan\nahead\n");
    vim.key(CMD_A, &mut shapes, &mut ink);
    until(&mut vim, &mut shapes, &mut ink, |v, _| v.selects() == Some(true));
    assert!(vim.copy(true, &shapes), "a cut");
    until(&mut vim, &mut shapes, &mut ink, |_, s| s.typing_text().as_deref() == Some(""));
    vim.pasted("back", &shapes);
    until(&mut vim, &mut shapes, &mut ink, |_, s| s.typing_text().as_deref() == Some("back"));
}

#[test]
fn a_yank_with_clipboard_unnamedplus_reaches_the_clipboard() {
    let Some((mut vim, _dir, clip)) = clipped() else { return };
    let mut ink = Ink::named("t");
    let mut shapes = opened("plan ahead", &mut ink);
    vim.typed(":set clipboard=unnamedplus", &shapes);
    vim.key(Chord::new(0, '\r'), &mut shapes, &mut ink);
    vim.typed("0yiw", &shapes);
    clip_holds(&mut vim, &mut shapes, &mut ink, &clip, "plan");
}

#[test]
fn new_texts_key_is_the_apps_and_a_command_chord_too() {
    let Some((mut vim, _dir)) = vim(READY) else { return };
    let mut ink = Ink::named("t");
    let mut shapes = new_text(&mut ink);
    let next = Chord::new(crate::commands::CTRL, '\r');
    shapes.set_next(Some(next));
    assert_eq!(vim.key(next, &mut shapes, &mut ink), Some(false));
    until(&mut vim, &mut shapes, &mut ink, |_, s| grid(s).is_some());
    let shown = shapes.shown(HERE, Vec::new(), 1.0, &Theme::default());
    let hint = shown.iter().find_map(|item| match &item.shape {
        Shape::Text { text, .. } if text.starts_with("esc in normal mode") => Some(text.clone()),
        _ => None,
    });
    assert_eq!(
        hint.as_deref(),
        Some("esc in normal mode, :q or a tap outside ends the text · ⌃↩ new text"),
        "the hint names the ways out"
    );
    assert_eq!(vim.key(Chord::new(crate::commands::CMD, 'z'), &mut shapes, &mut ink), Some(false));
    shapes.set_next(None);
    assert_eq!(vim.key(next, &mut shapes, &mut ink), Some(true), "unbound, it is Nvim's");
}

#[test]
fn a_press_and_a_drag_in_the_grid_are_nvims_mouse() {
    let Some((mut vim, _dir, clip)) = clipped() else { return };
    let mut ink = Ink::named("t");
    let mut shapes = opened("plan ahead", &mut ink);
    until(&mut vim, &mut shapes, &mut ink, |_, s| grid(s).is_some());
    // A cell is 12 wide and 24 tall at size 24: the fifth cell of the first row.
    let cell = |col: f64| Point { x: col * 12.0 + 1.0, y: 5.0 };
    assert!(!vim.press(HERE, (Point { x: 2000.0, y: 5.0 }, 8.0), false, &mut shapes), "outside");
    assert!(!vim.drag(cell(1.0), true, &shapes), "no press is down");
    assert!(vim.press(HERE, (cell(4.0), 8.0), false, &mut shapes));
    assert!(vim.drag(cell(4.0), true, &shapes));
    vim.typed("iX", &shapes);
    vim.key(Chord::new(0, '\u{1b}'), &mut shapes, &mut ink);
    // Nvim holds an escape for 'ttimeoutlen' to see whether a key follows it.
    until(&mut vim, &mut shapes, &mut ink, |_, s| {
        s.typing_text().as_deref() == Some("planX ahead")
            && grid(s).is_some_and(|(g, _)| g.mode_name == "normal")
    });
    assert!(vim.press(HERE, (cell(0.0), 8.0), false, &mut shapes));
    assert!(vim.drag(cell(2.0), false, &shapes));
    assert!(vim.drag(cell(3.0), true, &shapes));
    until(&mut vim, &mut shapes, &mut ink, |v, _| v.selects() == Some(true));
    vim.copy(false, &shapes);
    clip_holds(&mut vim, &mut shapes, &mut ink, &clip, "plan");
}

#[test]
fn a_double_tap_on_a_word_opens_nvim_in_normal_mode_with_its_cursor_on_that_cell() {
    let Some((mut vim, _dir)) = vim(READY) else { return };
    let mut ink = Ink::named("t");
    let frame = Rect { x: 0.0, y: 0.0, width: 200.0, height: 48.0 };
    let shape = Shape::Text { frame, text: "one\ntwo three".into(), size: 24.0, column: Some(1e7) };
    let items = [Cow::Owned(Item::plain(ItemId(7), RED, shape))];
    let mut shapes = Shapes::default();
    // Cells are 12 wide: x 100 is cell 8 of the second row, the "h" of three.
    let press = |when| Press { at: Point { x: 100.0, y: 30.0 }, when, zoom: 1.0, shift: false };
    for when in [0.0, 0.2] {
        shapes.down((Tool::Select, RED), HERE, press(when), &items, &mut ink);
    }
    assert!(shapes.tapped(Point { x: 100.0, y: 30.0 }));
    vim.sync(&shapes);
    until(&mut vim, &mut shapes, &mut ink, |_, s| {
        grid(s).is_some_and(|(g, _)| g.mode_name == "normal" && g.cursor == (1, 8))
    });
}
