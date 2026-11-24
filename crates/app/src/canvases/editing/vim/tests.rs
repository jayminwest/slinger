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
    let item = Item { id: ItemId(7), colour: RED, shape };
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
