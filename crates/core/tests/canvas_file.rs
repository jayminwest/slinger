//! The canvas file codec, through its public interface only.
#![cfg(test)]

use slinger_core::canvas::file::{Change, Header, Line, VERSION, read, snap};
use slinger_core::canvas::{
    Action, Applied, Author, Canvas, Colour, End, History, Item, ItemId, Op, Page, PageId, Point,
    Rect, Shape, Time,
};

use std::fmt::Write as _;

const FIXTURE: &str = include_str!("fixtures/canvas-v1.canvas");

/// A small deterministic generator (xorshift64), so a failure replays.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, n: usize) -> usize {
        usize::try_from(self.next() % n as u64).unwrap()
    }

    /// Any value in about ±10 000 units, rarely on the tenths grid.
    #[expect(clippy::cast_precision_loss, reason = "test values are small")]
    fn coord(&mut self) -> f64 {
        (self.next() % 20_000_000) as f64 / 1000.0 - 10_000.0
    }

    fn point(&mut self) -> Point {
        Point::new(self.coord(), self.coord())
    }

    fn pick<T: Clone>(&mut self, from: &[T]) -> T {
        from[self.below(from.len())].clone()
    }
}

fn colour(rng: &mut Rng) -> Colour {
    let b = rng.next().to_le_bytes();
    if b[4].is_multiple_of(2) {
        Colour::rgb(b[0], b[1], b[2])
    } else {
        Colour { r: b[0], g: b[1], b: b[2], a: b[3] }
    }
}

fn shape(rng: &mut Rng, items: &[ItemId]) -> Shape {
    let frame = |rng: &mut Rng| {
        let size = rng.point();
        Rect::new(rng.coord(), rng.coord(), size.x.abs(), size.y.abs())
    };
    match rng.below(4) {
        0 => Shape::Stroke {
            points: (0..=rng.below(30)).map(|_| rng.point()).collect(),
            colour: colour(rng),
            width: rng.coord().abs() / 100.0 + 0.001,
        },
        1 => Shape::Text {
            text: rng.pick(&["", "hi", "a \"quoted\"\nline", "ünïcode ✓", "\\ \t"]).to_string(),
            frame: frame(rng),
            colour: colour(rng),
        },
        2 => Shape::Box { frame: frame(rng), colour: colour(rng) },
        _ => {
            let end = |rng: &mut Rng| {
                let item = (!items.is_empty() && rng.below(2) == 0).then(|| rng.pick(items));
                End { at: rng.point(), item }
            };
            Shape::Arrow { from: end(rng), to: end(rng), colour: colour(rng) }
        }
    }
}

fn action(rng: &mut Rng, c: &Canvas) -> Action {
    let pages: Vec<PageId> = c.pages().map(Page::id).chain([PageId::new(99)]).collect();
    let items: Vec<ItemId> =
        pages.iter().flat_map(|p| c.items(*p).map(Item::id)).chain([ItemId::new(999)]).collect();
    let names = ["a", "b", " c ", "Page 1", "d"];
    match rng.below(10) {
        0..=3 => Action::Add { page: rng.pick(&pages), shape: shape(rng, &items) },
        4 => Action::Edit { item: rng.pick(&items), shape: shape(rng, &items) },
        5 => Action::Move { item: rng.pick(&items), by: rng.point() },
        6 => Action::Remove { item: rng.pick(&items) },
        7 => Action::PageAdd { name: rng.pick(&names).into() },
        8 => Action::PageRename { page: rng.pick(&pages), name: rng.pick(&names).into() },
        _ => Action::PageRemove { page: rng.pick(&pages) },
    }
}

/// Applies `op` (snapped) to `canvas` and appends its line, as the daemon will.
fn record(canvas: &mut Canvas, file: &mut String, n: &mut u64, op: Op) -> Option<Line> {
    let op = snap(op);
    let applied = canvas.apply(op.clone()).ok()?;
    let line = Line::of(*n, &op, applied)?;
    file.push_str(&line.encode());
    *n += 1;
    Some(line)
}

/// A canvas as the file sees it: its pages in order, then every item whole,
/// at tenths. A move adds two values on the tenths grid in `f64`, which can
/// land a hair off it in memory; the file holds the grid value.
fn seen(c: &Canvas) -> String {
    let mut out = String::new();
    for page in c.pages() {
        writeln!(out, "{} {}", page.id(), page.name()).unwrap();
        for item in c.items(page.id()) {
            let op = Op {
                by: Author::User,
                at: Time(0),
                action: Action::Restore { item: item.clone() },
            };
            out += &Line::of(0, &op, Applied::Changed).unwrap().encode();
        }
    }
    out
}

/// Appends the lines for ops that changed the canvas.
fn append(ops: &[(Op, Applied)], file: &mut String, n: &mut u64, lines: &mut Vec<Line>) {
    for (op, applied) in ops {
        if let Some(line) = Line::of(*n, op, *applied) {
            file.push_str(&line.encode());
            lines.push(line);
            *n += 1;
        }
    }
}

#[test]
fn generated_canvases_round_trip_through_the_file() {
    let mut rng = Rng(0x5eed_cafe_f00d_d00d);
    let mut every = String::new();
    for _ in 0..300 {
        let (mut canvas, mut history) = (Canvas::new(), History::new());
        let (mut file, mut n, mut lines) = (Header::new("c").encode(), 1, Vec::new());
        for _ in 0..rng.below(100) {
            let by = rng.pick(&[
                Author::User,
                Author::Agent("scout".into()),
                Author::Agent("a:b".into()),
            ]);
            let at = Time(rng.next() % 250_000_000_000_000);
            let done: Vec<(Op, Applied)> = match rng.below(10) {
                0 | 1 => history.undo(&mut canvas, &by, at),
                2 => history.redo(&mut canvas, &by, at),
                _ => {
                    // One step of one or two actions, as a lasso move is.
                    let ops: Vec<Op> = (0..=rng.below(2))
                        .map(|_| snap(Op { by: by.clone(), at, action: action(&mut rng, &canvas) }))
                        .collect();
                    let actions = ops.iter().map(|op| op.action.clone()).collect();
                    let applied = history.apply(&mut canvas, &by, at, actions).unwrap_or_default();
                    append(
                        &ops.into_iter().zip(applied).collect::<Vec<_>>(),
                        &mut file,
                        &mut n,
                        &mut lines,
                    );
                    continue;
                }
            }
            .into_iter()
            .map(|op| (op, Applied::Changed))
            .collect();
            append(&done, &mut file, &mut n, &mut lines);
        }
        let read = read(file.as_bytes()).unwrap();
        assert_eq!(read.header, Header::new("c"));
        assert_eq!((read.torn, read.next()), (0, n));
        assert_eq!(read.lines, lines);
        assert_eq!(seen(&read.canvas().unwrap()), seen(&canvas));
        let again: String = read.lines.iter().map(Line::encode).collect();
        assert_eq!(Header::new("c").encode() + &again, file, "encoding is stable");
        every += &file;
    }
    for op in ["page.add", "page.rename", "page.remove", "page.restore", "add", "remove", "move"]
        .into_iter()
        .chain(["set", "restore"])
    {
        assert!(every.contains(&format!(r#""op":"{op}""#)), "no {op} was generated");
    }
}

#[test]
fn the_stored_v1_fixture_still_decodes() {
    let file = read(FIXTURE.as_bytes()).unwrap();
    assert_eq!(file.header, Header { id: "c1".into(), first: 1 });
    assert_eq!((file.lines.len(), file.torn, file.next()), (18, 0, 19));
    let unknown: Vec<u64> =
        file.lines.iter().filter(|l| matches!(l, Line::Unknown { .. })).map(Line::n).collect();
    assert_eq!(unknown, [8, 9, 10], "an image item, page.move and an unknown op");

    let c = file.canvas().unwrap();
    let pages: Vec<_> = c.pages().map(|p| (p.id().get(), p.name().to_string())).collect();
    assert_eq!(pages, [(1, "Page 1".into()), (3, "scratch".into()), (2, "specs".into())]);
    let ids = |p| c.items(PageId::new(p)).map(|i| i.id().get()).collect::<Vec<_>>();
    assert_eq!((ids(1), ids(2)), (vec![2, 3, 6], vec![4]));

    let item = |n| c.item(ItemId::new(n)).unwrap();
    let black = Colour::BLACK;
    assert_eq!(
        item(2).shape(),
        &Shape::Box { frame: Rect::new(180.0, 700.0, 90.0, 60.0), colour: black }
    );
    let text = item(3);
    assert_eq!(text.author(), &Author::Agent("scout".into()));
    assert_eq!(text.time(), Time(1_794_328_930_400));
    let frame = Rect::new(280.0, 700.0, 200.0, 30.0);
    assert_eq!(text.shape(), &Shape::Text { text: "Two pages.".into(), frame, colour: black });
    let arrow = Shape::Arrow {
        from: End::on(Point::new(0.0, 0.0), ItemId::new(2)),
        to: End::free(Point::new(50.0, 0.0)),
        colour: Colour { r: 255, g: 0, b: 0, a: 128 },
    };
    assert_eq!(item(4).shape(), &arrow);
    assert!(c.item(ItemId::new(1)).is_none(), "the stroke was removed");

    // A known line with no unknown field is written back byte for byte.
    for (line, text) in file.lines.iter().zip(FIXTURE.lines().skip(1)) {
        if !text.contains("\"why\"") && !text.contains("\"shadow\"") {
            assert_eq!(line.encode(), format!("{text}\n"));
        }
    }
}

#[test]
fn the_stroke_points_are_deltas_in_tenths() {
    let file = read(FIXTURE.as_bytes()).unwrap();
    let Line::Op { change: Change::Add { shape: Shape::Stroke { points, width, .. }, .. }, .. } =
        &file.lines[1]
    else {
        panic!("line 3 is a stroke")
    };
    let want = [(209.6, 738.6), (206.7, 732.2), (206.7, 728.2), (210.7, 736.2)];
    assert_eq!(*points, want.map(|(x, y)| Point::new(x, y)));
    assert!((width - 2.0).abs() < f64::EPSILON);
}

#[test]
fn a_torn_last_line_is_cut_and_everything_before_it_loads() {
    let whole = read(FIXTURE.as_bytes()).unwrap();
    for tail in [r#"{"n":16,"t":"2026-11"#, "{\"n\":16,\"t\":\n", "\u{0}\u{0}\n"] {
        let file = read(format!("{FIXTURE}{tail}").as_bytes()).unwrap();
        assert_eq!(file.torn, tail.len(), "{tail:?}");
        assert_eq!(file.lines, whole.lines);
        assert_eq!(file.canvas().unwrap(), whole.canvas().unwrap());
    }
}

fn error(bytes: &str) -> (String, String) {
    let err = read(bytes.as_bytes()).unwrap_err();
    assert!(err.hint().is_some());
    (err.code().to_string(), err.to_string())
}

fn with_line(n: usize, text: &str) -> String {
    let mut lines: Vec<&str> = FIXTURE.lines().collect();
    lines[n - 1] = text;
    lines.join("\n") + "\n"
}

#[test]
fn a_bad_middle_line_is_corrupt_with_its_number() {
    let (code, msg) = error(&with_line(4, "{\"n\":3,"));
    assert_eq!(code, "canvas_corrupt");
    assert!(msg.contains("line 4"), "{msg}");
    let (code, msg) = error(&with_line(
        5,
        r#"{"n":4,"t":"2026-11-10T16:41:55.000Z","by":"user","op":"move","ids":["i1"]}"#,
    ));
    assert_eq!(code, "canvas_corrupt");
    assert!(msg.contains("line 5") && msg.contains("dx"), "{msg}");
    let (code, msg) =
        error(&with_line(3, r#"{"n":2,"t":"yesterday","by":"user","op":"remove","ids":["i1"]}"#));
    assert!(code == "canvas_corrupt" && msg.contains("line 3"), "{msg}");
}

#[test]
fn a_gap_in_n_is_corrupt() {
    let (code, msg) = error(&with_line(
        4,
        r#"{"n":4,"t":"2026-11-10T16:41:52.120Z","by":"user","op":"remove","ids":["i1"]}"#,
    ));
    assert_eq!(code, "canvas_corrupt");
    assert!(msg.contains("line 4") && msg.contains("expected operation 3"), "{msg}");
}

#[test]
fn a_newer_version_is_refused_before_any_line() {
    let header = format!(r#"{{"slinger":"canvas","v":{},"id":"c1","first":1}}"#, VERSION + 1);
    let (code, msg) = error(&with_line(1, &header));
    assert_eq!(code, "canvas_too_new");
    assert!(msg.contains('2'), "{msg}");
}

#[test]
fn a_missing_or_foreign_header_is_corrupt_at_line_1() {
    for bytes in [
        String::new(),
        r#"{"slinger":"canvas","v":1,"id":"c1","first":1}"#.into(),
        with_line(1, r#"{"tldraw":"canvas"}"#),
        with_line(1, r#"{"slinger":"canvas","v":0,"id":"c1","first":1}"#),
        with_line(1, r#"{"slinger":"canvas","v":1,"first":1}"#),
        with_line(1, r#"{"slinger":"canvas","v":1,"id":"c1","first":0}"#),
        with_line(1, "[]"),
    ] {
        let (code, msg) = error(&bytes);
        assert!(code == "canvas_corrupt" && msg.contains("line 1"), "{bytes:?}: {msg}");
    }
}

#[test]
fn an_unknown_operation_or_author_is_kept_whole_and_skipped() {
    let tag = r#"{"n":1,"op":"tag","anything":{"goes":[1,2]}}"#;
    let robot = r#"{"n":2,"t":"2026-11-10T16:41:46.982Z","by":"robot","op":"page.add","page":"p2","name":"x"}"#;
    let bytes = format!("{}{tag}\n{robot}\n", Header::new("c").encode());
    let file = read(bytes.as_bytes()).unwrap();
    assert_eq!(
        file.lines,
        [Line::Unknown { n: 1, text: tag.into() }, Line::Unknown { n: 2, text: robot.into() },]
    );
    assert_eq!(
        file.lines.iter().map(Line::encode).collect::<String>(),
        format!("{tag}\n{robot}\n")
    );
    assert_eq!(file.canvas().unwrap(), Canvas::new());
}

#[test]
fn unknown_fields_are_ignored() {
    let file = read(FIXTURE.as_bytes()).unwrap();
    let Line::Op { change, .. } = &file.lines[10] else { panic!("line 12 is known") };
    assert_eq!(change, &Change::Remove { items: vec![ItemId::new(1)] });
}

#[test]
fn a_file_may_start_above_1() {
    let mut canvas = Canvas::new();
    let header = Header { id: "c".into(), first: 41 };
    let (mut bytes, mut n) = (header.encode(), 41);
    let at = Time(0);
    for name in ["x", "y"] {
        let op = Op { by: Author::User, at, action: Action::PageAdd { name: name.into() } };
        record(&mut canvas, &mut bytes, &mut n, op).unwrap();
    }
    let file = read(bytes.as_bytes()).unwrap();
    assert_eq!(file.header, header);
    assert_eq!((file.lines[0].n(), file.next()), (41, 43));
    assert_eq!(file.canvas().unwrap(), canvas);
}

#[test]
fn ids_that_do_not_rise_are_corrupt_on_replay() {
    let line = |n, id| {
        format!(
            r#"{{"n":{n},"t":"2026-11-10T16:41:46.982Z","by":"user","op":"page.add","page":"p{id}","name":"p{id}"}}"#
        )
    };
    let bytes = format!("{}{}\n{}\n", Header::new("c").encode(), line(1, 3), line(2, 3));
    let err = read(bytes.as_bytes()).unwrap().canvas().unwrap_err();
    assert_eq!(err.code(), "canvas_corrupt");
    assert!(err.to_string().contains("line 3"), "{err}");
}

#[test]
fn an_op_that_changed_nothing_has_no_line() {
    let mut canvas = Canvas::new();
    let op = Op { by: Author::User, at: Time(0), action: Action::Remove { item: ItemId::new(7) } };
    let applied = canvas.apply(op.clone()).unwrap();
    assert_eq!(applied, Applied::Unchanged);
    assert_eq!(Line::of(1, &op, applied), None);
    let add = Op { action: Action::PageAdd { name: "x".into() }, ..op };
    assert_eq!(Line::of(1, &add, Applied::Changed), None, "the result does not fit the op");
}

#[test]
fn snap_rounds_to_tenths_and_leaves_what_the_canvas_rejects() {
    let op = |action| Op { by: Author::User, at: Time(0), action };
    let page = PageId::new(1);
    let thin =
        Shape::Stroke { points: vec![Point::new(1.26, -1.25)], colour: Colour::BLACK, width: 0.01 };
    let snapped =
        Shape::Stroke { points: vec![Point::new(1.3, -1.3)], colour: Colour::BLACK, width: 0.1 };
    assert_eq!(
        snap(op(Action::Add { page, shape: thin })).action,
        Action::Add { page, shape: snapped.clone() }
    );
    assert_eq!(
        snap(op(Action::Add { page, shape: snapped.clone() })).action,
        Action::Add { page, shape: snapped }
    );
    let nan = Action::Move { item: ItemId::new(1), by: Point::new(f64::NAN, 0.0) };
    assert!(matches!(snap(op(nan)).action, Action::Move { by, .. } if by.x.is_nan()));
    let item = ItemId::new(1);
    let moved = snap(op(Action::Move { item, by: Point::new(0.04, 0.05) })).action;
    assert_eq!(moved, Action::Move { item, by: Point::new(0.0, 0.1) });
    let rename = Action::PageRename { page, name: "x".into() };
    assert_eq!(snap(op(rename.clone())).action, rename);
}
