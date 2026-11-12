use super::*;

fn stroke(canvas: &str, x: f64) -> Stroke {
    Stroke {
        canvas: canvas.into(),
        page: "p1".into(),
        colour: Rgb { r: 0, g: 10, b: 255 },
        width: 2.5,
        points: vec![Point { x, y: 1.25 }, Point { x: x + 1.0, y: -2.0 }],
    }
}

fn kept(canvas: &str, x: f64, once: Option<&str>) -> KeptStroke {
    KeptStroke { stroke: stroke(canvas, x), once: once.map(str::to_string) }
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
    let good = write(&kept("notes", 1.0, None));
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
fn two_apps_keep_a_file_each_and_the_next_start_takes_both() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("unsent-ink.json");
    let (mut one, mut two) = (Unsent::at(path.clone()), Unsent::at(path.clone()));
    assert_eq!((one.claim(), two.claim()), (Ok(vec![]), Ok(vec![])));
    let lone = Unsent::at(path.clone());
    assert!(lone.save(&[kept("x", 9.0, None)]).is_err(), "not taken: it writes nothing");
    one.save(&[kept("notes", 1.0, Some("a.1"))]).unwrap();
    two.save(&[kept("notes", 2.0, Some("b.1"))]).unwrap();
    assert_eq!(Unsent::at(path.clone()).load().unwrap(), [kept("notes", 1.0, Some("a.1"))]);
    let second = dir.path().join("unsent-ink.2.json");
    assert_eq!(Unsent::at(second.clone()).load().unwrap(), [kept("notes", 2.0, Some("b.1"))]);

    let mut three = Unsent::at(path.clone());
    assert_eq!(three.claim(), Ok(vec![]), "both held: the third file, and none of theirs");
    drop((one, two, three));
    let mut next = Unsent::at(path.clone());
    let both = [kept("notes", 1.0, Some("a.1")), kept("notes", 2.0, Some("b.1"))];
    assert_eq!(next.claim().unwrap(), both);
    assert_eq!(Unsent::at(path).load().unwrap(), both, "in its own file now");
    assert!(!second.exists());
}

#[test]
fn the_file_is_beside_the_daemon_s_record() {
    let located = Unsent::locate().map(|f| f.path);
    assert!(located.is_none_or(|p| p.ends_with("slinger/unsent-ink.json")));
}
