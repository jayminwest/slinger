//! Strokes drawn and changes made while the daemon is down survive the app
//! quitting.

use slinger_control::Client;

use super::*;
use crate::backend::testing::wait_until;
use crate::backend::tests::{open_notes, with_store};
use crate::config::Rgb;
use crate::layout::Point;
use crate::scene::canvas::{ItemId, Shape};
use crate::scene::ink::{Kept, PENDING};

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
        let [Kept { once: Some(once), change, .. }] = kept.as_slice() else { panic!("{kept:?}") };
        // What the outbox sent as the app quit, written, the answer lost.
        let ops = crate::mirror::ops(change);
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

/// slinger-1d05ae: a second app running beside the first keeps nothing and
/// says so, rather than write over the first's strokes.
#[test]
fn a_second_app_keeps_nothing_and_says_so() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("unsent-ink.json");
    let down =
        || Canvases::new(Client::new("/nonexistent/x.sock")).keeping(Unsent::at(path.clone()));
    let mut one = down();
    let mut two = down();
    assert!(one.kept.as_ref().unwrap().problem().is_none());
    let problem = two.kept.as_ref().unwrap().problem().unwrap();
    assert!(problem.message.contains("another slinger app keeps it"), "{problem:?}");
    draw(&mut one, "notes", 1.0);
    one.poll();
    draw(&mut two, "notes", 2.0);
    two.poll();
    draw(&mut one, "notes", 4.0);
    one.poll();
    let kept = Unsent::at(path.clone()).load().unwrap();
    let x = |k: &Kept| match &k.change {
        Change::Add(stroke) => stroke.points[0].x,
        other => panic!("{other:?}"),
    };
    let xs: Vec<f64> = kept.iter().map(x).collect();
    assert_eq!(xs, [1.0, 4.0], "the first app's alone");
}

/// slinger-e7fc87: a move of a written stroke, an arrow bound to a stroke
/// not yet written and the erase of another, all made while the daemon is
/// down, are kept with the strokes and sent in order after the next start,
/// naming the strokes by the ids the daemon makes then.
#[test]
fn changes_made_while_the_daemon_is_down_are_sent_after_the_next_start() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("unsent-ink.json");
    with_store(None, |daemon| {
        let _home = open_notes(daemon);
        let mut quit = Canvases::new(daemon.clone()).keeping(Unsent::at(path.clone()));
        draw(&mut quit, "notes", 1.0);
        wait_until(|| {
            quit.poll();
            quit.ink.settled("notes")
        });
        let written = ItemId(1);
        quit.outbox.reconnect(Client::new("/nonexistent/x.sock"));
        let (canvas, page) = (String::from("notes"), String::from("p1"));
        let by = Point { x: 10.0, y: 0.0 };
        let ids = vec![written];
        quit.ink.push(Change::Move {
            canvas: canvas.clone(),
            page: page.clone(),
            ids,
            by,
            follow: vec![],
        });
        draw(&mut quit, "notes", 3.0);
        let kept = PENDING + quit.ink.unsent().last().unwrap().0;
        let (from, to) = (Point { x: 0.0, y: 0.0 }, Point { x: 3.0, y: 1.0 });
        let bound = [None, Some(ItemId(kept))];
        let shape = Shape::Arrow { from, to, width: 2.0, bound };
        quit.ink.push(Change::Put {
            canvas: canvas.clone(),
            page: page.clone(),
            colour: BLUE,
            shape,
        });
        draw(&mut quit, "notes", 5.0);
        let erased = PENDING + quit.ink.unsent().last().unwrap().0;
        quit.ink.push(Change::Remove { canvas, page, ids: vec![ItemId(erased)] });
        quit.poll();
        assert_eq!(Unsent::at(path.clone()).load().unwrap().len(), 5, "kept before any answer");
        drop(quit);
        assert_eq!(strokes(daemon), [10], "nothing sent but the first stroke");

        let mut canvases = Canvases::new(daemon.clone()).keeping(Unsent::at(path.clone()));
        wait_until(|| {
            canvases.poll();
            canvases.ink.settled("notes") && !path.exists()
        });
        let items = daemon.canvas_show("notes", None).unwrap().items;
        let kinds: Vec<_> = items.iter().map(|i| (i["id"].clone(), i["kind"].clone())).collect();
        assert_eq!(
            kinds,
            [
                (serde_json::json!("i1"), serde_json::json!("stroke")),
                (serde_json::json!("i2"), serde_json::json!("stroke")),
                (serde_json::json!("i3"), serde_json::json!("arrow")),
            ],
            "the erased stroke made and taken away again"
        );
        assert_eq!(items[0]["pts"][0], 110, "moved");
        assert_eq!(items[2]["to"], "i2", "bound to the kept stroke, by the id it was made under");
    });
}
