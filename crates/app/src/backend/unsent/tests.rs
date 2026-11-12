use super::*;
use crate::config::Rgb;
use crate::layout::{Point, Rect};
use crate::scene::canvas::{Item, ItemId, Shape};
use crate::scene::ink::{Carried, Change, Stroke};
use crate::scene::pages::PageOp;

fn stroke(canvas: &str, x: f64) -> Stroke {
    Stroke {
        canvas: canvas.into(),
        page: "p1".into(),
        colour: Rgb { r: 0, g: 10, b: 255 },
        width: 2.5,
        points: vec![Point { x, y: 1.25 }, Point { x: x + 1.0, y: -2.0 }],
    }
}

fn kept(canvas: &str, x: f64, once: Option<&str>) -> Kept {
    let once = once.map(str::to_string);
    Kept { key: None, after: None, once, change: Change::Add(stroke(canvas, x)) }
}

#[test]
fn strokes_kept_come_back_in_order_and_none_left_removes_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let mut file = Unsent::at(dir.path().join("deep/unsent-ink.json"));
    assert_eq!(file.claim(), Ok(vec![]), "no file: none kept");
    let strokes = [kept("notes", 0.5, Some("1-2.3")), kept("other", 3.0, None)];
    file.save(&strokes).unwrap();
    assert_eq!(file.load().unwrap(), strokes);
    file.save(&strokes[1..]).unwrap();
    assert_eq!(file.load().unwrap(), strokes[1..]);
    file.save(&[]).unwrap();
    assert!(!dir.path().join("deep/unsent-ink.json").exists());
    assert!(!dir.path().join("deep/unsent-ink.json.new").exists());
    file.save(&[]).unwrap();
}

#[test]
fn a_stroke_that_does_not_read_is_left_out_and_a_bad_file_is_a_problem() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("unsent-ink.json");
    let good = change::write(&kept("notes", 1.0, None));
    let odd = json!({"canvas": "notes", "page": "p1", "colour": [0, 0, 0],
        "width": 2.0, "points": [1.0]});
    fs::write(&path, json!({"version": 1, "strokes": [odd, good, 7]}).to_string()).unwrap();
    assert_eq!(Unsent::at(path.clone()).load().unwrap(), [kept("notes", 1.0, None)]);
    fs::write(&path, "{nope").unwrap();
    let problem = Unsent::at(path.clone()).claim().unwrap_err();
    assert!(problem.message.starts_with(&path.display().to_string()), "{problem:?}");
    assert!(problem.hint.is_some());
}

#[test]
fn one_app_at_a_time_keeps_the_file_and_the_next_start_takes_it() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("deep/unsent-ink.json");
    let mut one = Unsent::at(path.clone());
    assert_eq!(one.claim(), Ok(vec![]));
    one.save(&[kept("notes", 1.0, Some("a.1"))]).unwrap();
    let mut two = Unsent::at(path.clone());
    let problem = two.claim().unwrap_err();
    assert!(problem.message.contains("another slinger app keeps it"), "{problem:?}");
    assert!(two.save(&[kept("notes", 2.0, None)]).is_err(), "not held: it writes nothing");
    assert!(Unsent::at(path.clone()).save(&[]).is_err(), "never claimed: nor does this");
    assert_eq!(Unsent::at(path.clone()).load().unwrap(), [kept("notes", 1.0, Some("a.1"))]);

    drop((one, two));
    let mut next = Unsent::at(path);
    assert_eq!(next.claim().unwrap(), [kept("notes", 1.0, Some("a.1"))]);
}

#[test]
fn the_file_is_beside_the_daemon_s_record() {
    let located = Unsent::locate().map(|f| f.path);
    assert!(located.is_none_or(|p| p.ends_with("slinger/unsent-ink.json")));
}

/// slinger-e7fc87: every kind of change a quit can leave unsent comes back
/// as it was kept, its keys too; a file of strokes alone, as the first
/// files were, reads as strokes.
#[test]
fn every_change_comes_back_as_kept_and_a_file_of_strokes_alone_reads() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("unsent-ink.json");
    let (canvas, page) = (String::from("notes"), String::from("p1"));
    let colour = Rgb { r: 0, g: 10, b: 255 };
    let frame = Rect { x: 1.5, y: -2.0, width: 30.0, height: 4.5 };
    let item = |id, shape| Item { id: ItemId(id), colour, shape };
    let at = |x, y| Point { x, y };
    let arrow = Shape::Arrow {
        from: at(0.0, 0.0),
        to: at(5.5, 1.0),
        width: 2.0,
        bound: [Some(ItemId(3)), None],
    };
    let live = Shape::Live { frame, name: "board".into(), open: "pick-slot#2".into() };
    let pending = ItemId(crate::scene::ink::PENDING + 4);
    let carried = Carried {
        item: item(5, Shape::Stroke { points: vec![at(1.0, 2.0), at(3.0, 4.5)], width: 2.0 }),
        object: json!({"page": "p2", "kind": "stroke", "pts": [10, 20, 20, 25]}),
    };
    let changes = [
        Change::Add(stroke("notes", 0.25)),
        Change::Remove {
            canvas: canvas.clone(),
            page: page.clone(),
            ids: vec![ItemId(7), pending],
        },
        Change::Move {
            canvas: canvas.clone(),
            page: page.clone(),
            ids: vec![ItemId(1)],
            by: at(0.25, -3.0),
            follow: vec![item(2, arrow.clone())],
        },
        Change::Put {
            canvas: canvas.clone(),
            page: page.clone(),
            colour,
            shape: Shape::Box { frame, width: 2.0 },
        },
        Change::Set { canvas: canvas.clone(), page: page.clone(), id: pending, shape: arrow },
        Change::Reshape { canvas: canvas.clone(), page: page.clone(), items: vec![item(9, live)] },
        Change::Carry {
            canvas: canvas.clone(),
            page: "p2".into(),
            items: vec![carried],
            gone: vec![ItemId(5)],
        },
        Change::Page { canvas: canvas.clone(), op: PageOp::Add { name: "two".into() } },
        Change::Page {
            canvas: canvas.clone(),
            op: PageOp::Rename { page: "p2".into(), name: "2".into() },
        },
        Change::Page { canvas, op: PageOp::Remove { page } },
    ];
    let kept: Vec<Kept> = (0..)
        .zip(changes)
        .map(|(key, change)| Kept {
            key: Some(key),
            after: key.checked_sub(1),
            once: Some(format!("run.{key}")),
            change,
        })
        .collect();
    let mut file = Unsent::at(path.clone());
    file.claim().unwrap();
    file.save(&kept).unwrap();
    let back = file.load().unwrap();
    assert_eq!(back.len(), kept.len());
    for (back, kept) in back.iter().zip(&kept) {
        assert_eq!(back, kept);
    }

    let strokes = json!({"version": 1, "strokes": [change::write(&kept[0])]});
    fs::write(&path, strokes.to_string()).unwrap();
    assert_eq!(file.load().unwrap(), kept[..1]);
}
