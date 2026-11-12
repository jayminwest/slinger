//! Strokes drawn while the daemon is down survive the app quitting.

use slinger_control::Client;

use super::*;
use crate::backend::testing::wait_until;
use crate::backend::tests::{open_notes, with_store};
use crate::config::Rgb;
use crate::layout::Point;

const BLUE: Rgb = Rgb { r: 0, g: 0, b: 255 };

/// A stroke on page p1 of `canvas` from `x`, finished.
fn draw(canvases: &mut Canvases, canvas: &str, x: f64) {
    canvases.ink.down(canvas, "p1", BLUE, Point { x, y: 1.0 });
    canvases.ink.drag(Point { x: x + 1.0, y: 2.0 });
    assert!(canvases.ink.up(None));
}

/// The first x of each stroke the daemon holds on `notes`, in tenths.
fn strokes(daemon: &Client) -> Vec<serde_json::Value> {
    let items = daemon.canvas_show("notes", None).unwrap().items;
    items.iter().map(|i| i["pts"][0].clone()).collect()
}

#[test]
fn strokes_drawn_while_the_daemon_is_down_are_sent_after_the_next_start() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("unsent-ink.json");
    let mut quit =
        Canvases::new(Client::new("/nonexistent/x.sock")).keeping(Unsent::at(path.clone()));
    draw(&mut quit, "notes", 1.0);
    draw(&mut quit, "gone", 2.0);
    draw(&mut quit, "notes", 3.0);
    quit.poll();
    assert_eq!(Unsent::at(path.clone()).load().unwrap().len(), 3, "kept before any answer");
    wait_until(|| {
        quit.poll();
        quit.problems().next().is_some()
    });
    assert_eq!(Unsent::at(path.clone()).load().unwrap().len(), 3, "not reached: still kept");
    drop(quit);

    with_store(None, |daemon| {
        let _home = open_notes(daemon);
        let mut canvases = Canvases::new(daemon.clone()).keeping(Unsent::at(path.clone()));
        draw(&mut canvases, "notes", 4.0);
        wait_until(|| {
            canvases.poll();
            strokes(daemon).len() == 3 && !path.exists()
        });
        assert_eq!(
            strokes(daemon),
            [10, 30, 40],
            "the kept first, in order; the gone canvas's dropped"
        );
    });
}

#[test]
fn a_file_that_cannot_be_read_is_a_problem_and_a_new_stroke_replaces_it() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("unsent-ink.json");
    std::fs::write(&path, "{nope").unwrap();
    let mut canvases =
        Canvases::new(Client::new("/nonexistent/x.sock")).keeping(Unsent::at(path.clone()));
    let problem = canvases.problems().next().unwrap();
    assert!(problem.message.starts_with(&path.display().to_string()), "{problem:?}");
    draw(&mut canvases, "notes", 1.0);
    canvases.poll();
    assert_eq!(Unsent::at(path).load().unwrap().len(), 1);
}
