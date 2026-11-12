//! Strokes drawn while the daemon is down survive the app quitting.

use slinger_control::Client;

use super::*;
use crate::backend::KeptStroke;
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

/// slinger-1d05ae: a stroke the daemon wrote but whose answer the app never
/// heard (it quit) is kept, and sent again at the next start under the same
/// key: the daemon does not write it twice.
#[test]
fn a_stroke_written_as_the_app_quit_is_not_written_twice() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("unsent-ink.json");
    with_store(None, |daemon| {
        let _home = open_notes(daemon);
        let mut quit = Canvases::new(daemon.clone()).keeping(Unsent::at(path.clone()));
        quit.outbox.reconnect(Client::new("/nonexistent/x.sock"));
        draw(&mut quit, "notes", 1.0);
        quit.poll();
        let kept = Unsent::at(path.clone()).load().unwrap();
        let [KeptStroke { once: Some(once), .. }] = kept.as_slice() else { panic!("{kept:?}") };
        // What the outbox sent as the app quit, written, the answer lost.
        let ops = crate::mirror::ops(&Change::Add(kept[0].stroke.clone()));
        let by = "user".to_string();
        let sent = slinger_control::CanvasOps {
            canvas: "notes".into(),
            by,
            ops,
            once: Some(once.clone()),
        };
        daemon.canvas_apply(sent).unwrap();
        drop(quit);

        let mut canvases = Canvases::new(daemon.clone()).keeping(Unsent::at(path.clone()));
        wait_until(|| {
            canvases.poll();
            canvases.ink.settled("notes") && !path.exists()
        });
        assert_eq!(strokes(daemon), [10], "written once");
    });
}

/// slinger-1d05ae: two apps running at once keep their strokes in a file
/// each, and neither writes over the other's.
#[test]
fn two_apps_keep_their_own_strokes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("unsent-ink.json");
    let down =
        || Canvases::new(Client::new("/nonexistent/x.sock")).keeping(Unsent::at(path.clone()));
    let (mut one, mut two) = (down(), down());
    draw(&mut one, "notes", 1.0);
    one.poll();
    draw(&mut two, "notes", 2.0);
    draw(&mut two, "notes", 3.0);
    two.poll();
    draw(&mut one, "notes", 4.0);
    one.poll();
    let xs = |file: &str| -> Vec<f64> {
        let kept = Unsent::at(dir.path().join(file)).load().unwrap();
        kept.iter().map(|k| k.stroke.points[0].x).collect()
    };
    assert_eq!(xs("unsent-ink.json"), [1.0, 4.0]);
    assert_eq!(xs("unsent-ink.2.json"), [2.0, 3.0]);
    drop((one, two));

    with_store(None, |daemon| {
        let _home = open_notes(daemon);
        let mut canvases = Canvases::new(daemon.clone()).keeping(Unsent::at(path.clone()));
        wait_until(|| {
            canvases.poll();
            strokes(daemon).len() == 4 && !path.exists()
        });
        assert_eq!(strokes(daemon), [10, 40, 20, 30], "its own first, then the other's");
    });
}
