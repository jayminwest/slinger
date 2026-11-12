use super::*;
use crate::scene::canvas::Mark;
use crate::scene::select::Select;

const RED: Rgb = Rgb { r: 255, g: 0, b: 0 };
const BLUE: Rgb = Rgb { r: 0, g: 0, b: 255 };
const HERE: (&str, &str) = ("main", "p1");
const THEME: Theme = Theme {
    background: Rgb { r: 0, g: 0, b: 0 },
    text: Rgb { r: 1, g: 1, b: 1 },
    muted: Rgb { r: 2, g: 2, b: 2 },
    error: Rgb { r: 3, g: 3, b: 3 },
    focus: Rgb { r: 4, g: 4, b: 4 },
    done: Rgb { r: 5, g: 5, b: 5 },
};

fn pt(x: f64, y: f64) -> Point {
    Point { x, y }
}

fn frame(x: f64, y: f64, w: f64, h: f64) -> Rect {
    Rect { x, y, width: w, height: h }
}

/// The frame of `text`'s glyphs from `(x, y)`: a text being typed shows,
/// and is sent, in it (the file keeps its own).
fn glyph(x: f64, y: f64, text: &str) -> Rect {
    glyphs::frame(pt(x, y), text, TEXT_SIZE, glyphs::UNWRAPPED)
}

fn item(id: u64, shape: Shape) -> Item {
    Item { id: ItemId(id), colour: BLUE, shape }
}

/// A box (i1), a text (i2), an arrow (i3) across the empty middle, and a
/// stroke of local ink over the right.
fn page() -> Vec<Item> {
    vec![
        item(1, Shape::Box { frame: frame(0.0, 0.0, 100.0, 100.0), width: 2.0 }),
        item(
            2,
            Shape::Text { frame: frame(300.0, 0.0, 96.0, 31.2), text: "plan".into(), size: 24.0 },
        ),
        item(
            3,
            Shape::Arrow {
                from: pt(150.0, 200.0),
                to: pt(250.0, 200.0),
                width: 2.0,
                bound: [None; 2],
            },
        ),
        item(
            crate::scene::ink::LOCAL,
            Shape::Box { frame: frame(500.0, 0.0, 50.0, 50.0), width: 2.0 },
        ),
    ]
}

fn shown(items: &[Item]) -> Vec<Cow<'_, Item>> {
    items.iter().map(Cow::Borrowed).collect()
}

fn press(x: f64, y: f64, when: f64) -> Press {
    Press { at: pt(x, y), when, zoom: 1.0 }
}

/// A press with `tool` at `at`, `when`, over `items`.
fn down(
    shapes: &mut Shapes,
    ink: &mut Ink,
    tool: Tool,
    at: (f64, f64),
    when: f64,
    items: &[Item],
) -> bool {
    shapes.down((tool, RED), HERE, press(at.0, at.1, when), &shown(items), ink)
}

/// A drag with `tool` from `a` to `b`, then the lift.
fn drag(
    shapes: &mut Shapes,
    ink: &mut Ink,
    tool: Tool,
    a: (f64, f64),
    b: (f64, f64),
    items: &[Item],
) {
    assert!(down(shapes, ink, tool, a, 0.0, items));
    assert!(shapes.drag(pt(f64::midpoint(a.0, b.0), f64::midpoint(a.1, b.1))));
    assert!(shapes.up(Some(pt(b.0, b.1)), &shown(items), ink));
}

/// Every change waiting in `ink`, oldest first.
fn changes(ink: &mut Ink) -> Vec<Change> {
    let mut all = Vec::new();
    while let Some((key, change)) = ink.next() {
        ink.answered(key, crate::scene::ink::Answer::Applied(1));
        all.push(change);
    }
    all
}

fn put(shape: Shape) -> Change {
    Change::Put { canvas: "main".into(), page: "p1".into(), colour: RED, shape }
}

#[test]
fn a_text_being_typed_says_its_canvas_and_page() {
    let (mut shapes, mut ink) = (Shapes::default(), Ink::default());
    assert_eq!(shapes.typing_on(), None);
    assert!(down(&mut shapes, &mut ink, Tool::Text, (10.0, 214.4), 0.0, &[]));
    assert_eq!(shapes.typing_on(), Some(HERE));
    assert!(shapes.typed("\u{1b}", &mut ink));
    assert_eq!(shapes.typing_on(), None);
}

#[test]
fn a_tap_places_a_text_the_keyboard_types_and_escape_ends_as_one_add() {
    let (mut shapes, mut ink) = (Shapes::default(), Ink::default());
    assert!(!shapes.typed("x", &mut ink), "nothing to type into");
    assert!(down(&mut shapes, &mut ink, Tool::Text, (10.0, 214.4), 0.0, &[]));
    assert!(shapes.editing());
    assert!(shapes.typed("Hi", &mut ink));
    assert!(shapes.typed("\u{f700}\t", &mut ink), "an arrow key and a tab type nothing");
    assert!(shapes.typed("x\u{7f}", &mut ink), "delete takes the last back");
    assert!(shapes.typed("\ryou", &mut ink));
    let showing = shapes.shown(HERE, Vec::new(), 1.0, &THEME);
    let [Cow::Owned(Item { colour: RED, shape: Shape::Text { text, frame: at, .. }, .. }), ..] =
        showing.as_slice()
    else {
        panic!("{showing:?}")
    };
    assert_eq!(text, "Hi\nyou");
    assert_eq!((at.x, at.y), (10.0, 200.0), "the tap is the middle of the first line");
    assert!(shapes.shown(("main", "p2"), Vec::new(), 1.0, &THEME).is_empty(), "only on its page");
    assert!(changes(&mut ink).is_empty(), "nothing is sent while typing");

    assert!(shapes.typed("\u{1b}", &mut ink));
    assert!(!shapes.editing());
    let text = "Hi\nyou".to_string();
    let frame = glyph(10.0, 200.0, "Hi\nyou");
    assert_eq!(changes(&mut ink), vec![put(Shape::Text { frame, text, size: TEXT_SIZE })]);
}

#[test]
fn a_paste_goes_in_at_the_end_its_lines_as_lines_and_ends_with_the_text() {
    let (mut shapes, mut ink) = (Shapes::default(), Ink::default());
    assert!(!shapes.pasted("x"), "nothing to paste into");
    assert!(down(&mut shapes, &mut ink, Tool::Text, (10.0, 214.4), 0.0, &[]));
    assert!(shapes.typed("To do: ", &mut ink));
    assert!(shapes.pasted("buy milk\r\ncall\u{1b} Sam"), "a pasted escape is not a key");
    assert!(shapes.editing(), "the paste did not end the text");
    assert!(changes(&mut ink).is_empty(), "nothing is sent while typing");
    assert!(shapes.typed("\u{1b}", &mut ink));
    let sent = changes(&mut ink);
    let [Change::Put { shape: Shape::Text { text, .. }, .. }] = sent.as_slice() else {
        panic!("one add")
    };
    assert_eq!(text, "To do: buy milk\ncall Sam");
}

#[test]
fn a_tap_outside_ends_the_text_and_is_used_up_a_tap_inside_is_not() {
    let (mut shapes, mut ink) = (Shapes::default(), Ink::default());
    assert!(down(&mut shapes, &mut ink, Tool::Text, (0.0, 0.0), 0.0, &[]));
    shapes.typed("a", &mut ink);
    assert!(
        down(&mut shapes, &mut ink, Tool::Box, (10.0, 0.0), 1.0, &[]),
        "inside: typing goes on"
    );
    assert!(shapes.editing());
    assert!(down(&mut shapes, &mut ink, Tool::Text, (200.0, 200.0), 2.0, &[]));
    assert!(!shapes.editing(), "the tap only ended it, it placed no new text");
    assert_eq!(changes(&mut ink).len(), 1);
    assert!(!shapes.drag(pt(1.0, 1.0)), "no box started");

    // Blank text makes nothing.
    assert!(down(&mut shapes, &mut ink, Tool::Text, (0.0, 0.0), 3.0, &[]));
    shapes.typed("  \r", &mut ink);
    assert!(shapes.end(&mut ink));
    assert!(!shapes.end(&mut ink), "nothing left to end");
    assert!(changes(&mut ink).is_empty(), "unchanged");
}

/// slinger-a77a12: a press outside a text still being typed, with any tool
/// but the text tool, ends the text and is that tool's press too.
#[test]
fn a_press_outside_with_another_tool_ends_the_text_and_starts_its_gesture() {
    let word = || Shape::Text { frame: glyph(0.0, -14.4, "hi"), text: "hi".into(), size: 24.0 };
    for tool in [Tool::Box, Tool::Arrow] {
        let (mut shapes, mut ink) = (Shapes::default(), Ink::default());
        assert!(down(&mut shapes, &mut ink, Tool::Text, (0.0, 0.0), 0.0, &[]));
        shapes.typed("hi", &mut ink);
        drag(&mut shapes, &mut ink, tool, (200.0, 200.0), (300.0, 260.0), &[]);
        assert!(!shapes.editing(), "{tool:?}");
        let made = changes(&mut ink);
        assert_eq!(made.len(), 2, "{tool:?}: the text, then the shape: {made:?}");
        assert_eq!(made[0], put(word()), "{tool:?}");
        let Change::Put { shape, .. } = &made[1] else { panic!("{made:?}") };
        assert_eq!(matches!(shape, Shape::Arrow { .. }), tool == Tool::Arrow, "{shape:?}");
    }
    for tool in [Tool::Pen, Tool::Eraser, Tool::Lasso] {
        let (mut shapes, mut ink) = (Shapes::default(), Ink::default());
        assert!(down(&mut shapes, &mut ink, Tool::Text, (0.0, 0.0), 0.0, &[]));
        shapes.typed("hi", &mut ink);
        let used = down(&mut shapes, &mut ink, tool, (200.0, 200.0), 5.0, &[]);
        assert!(!used, "{tool:?}: the press goes on to the pen, the eraser or the lasso");
        assert!(!shapes.editing(), "{tool:?}");
        assert_eq!(changes(&mut ink), vec![put(word())], "{tool:?}");
    }
}

/// Escape ends the text; the next drag with the box tool is a box.
#[test]
fn after_escape_the_first_box_drag_is_a_box() {
    let (mut shapes, mut ink) = (Shapes::default(), Ink::default());
    assert!(down(&mut shapes, &mut ink, Tool::Text, (0.0, 0.0), 0.0, &[]));
    shapes.typed("hi\u{1b}", &mut ink);
    assert!(!shapes.editing());
    drag(&mut shapes, &mut ink, Tool::Box, (200.0, 200.0), (300.0, 260.0), &[]);
    let boxed = Shape::Box { frame: frame(200.0, 200.0, 100.0, 60.0), width: LINE_WIDTH };
    assert_eq!(
        changes(&mut ink),
        vec![
            put(Shape::Text { frame: glyph(0.0, -14.4, "hi"), text: "hi".into(), size: 24.0 }),
            put(boxed)
        ]
    );
}

#[test]
fn the_text_tool_on_a_text_edits_it_as_one_set_or_a_remove_when_emptied() {
    let items = page();
    let (mut shapes, mut ink) = (Shapes::default(), Ink::default());
    assert!(down(&mut shapes, &mut ink, Tool::Text, (310.0, 10.0), 0.0, &items));
    let showing = shapes.shown(HERE, shown(&items), 1.0, &THEME);
    assert!(!showing.iter().any(|i| i.id == ItemId(2)), "the text shows only as edited");
    let edited = showing.iter().find(|i| i.id == EDITING).unwrap();
    assert_eq!(edited.colour, BLUE, "it keeps its colour");
    assert!(matches!(&edited.shape, Shape::Text { text, .. } if text == "plan"));
    shapes.typed("s", &mut ink);
    shapes.typed("\u{1b}", &mut ink);
    let text = "plans".to_string();
    let frame = glyph(300.0, 0.0, "plans");
    let set = Change::Set {
        canvas: "main".into(),
        page: "p1".into(),
        id: ItemId(2),
        shape: Shape::Text { frame, text, size: TEXT_SIZE },
    };
    assert_eq!(changes(&mut ink), vec![set]);

    // Unchanged makes nothing; emptied removes it.
    assert!(down(&mut shapes, &mut ink, Tool::Text, (310.0, 10.0), 5.0, &items));
    assert!(shapes.end(&mut ink));
    assert!(changes(&mut ink).is_empty(), "no text in two taps");
    assert!(down(&mut shapes, &mut ink, Tool::Text, (310.0, 10.0), 10.0, &items));
    shapes.typed("\u{7f}\u{7f}\u{7f}\u{7f}\u{7f}\u{1b}", &mut ink);
    let removed = Change::Remove { canvas: "main".into(), page: "p1".into(), ids: vec![ItemId(2)] };
    assert_eq!(changes(&mut ink), vec![removed]);
}

#[test]
fn a_double_tap_with_the_lasso_or_the_select_tool_edits_once_the_pen_lifts_where_it_went_down() {
    let items = page();
    let (mut shapes, mut ink) = (Shapes::default(), Ink::default());
    // The second press may be a drag of what the first tap selected.
    for (tool, when) in [(Tool::Lasso, 0.5), (Tool::Select, 0.7)] {
        assert!(!down(&mut shapes, &mut ink, tool, (310.0, 10.0), when, &items), "one tap");
        assert!(!down(&mut shapes, &mut ink, tool, (312.0, 12.0), when + 0.1, &items), "{tool:?}");
        assert!(!shapes.editing());
        assert!(shapes.tapped(pt(313.0, 12.0)), "{tool:?}: lifted where it went down");
        assert!(shapes.editing());
        shapes.end(&mut ink);
    }
    assert!(!down(&mut shapes, &mut ink, Tool::Select, (310.0, 10.0), 0.9, &items));
    assert!(!down(&mut shapes, &mut ink, Tool::Select, (310.0, 10.0), 1.0, &items));
    assert!(!shapes.tapped(pt(410.0, 10.0)), "a drag: no edit");
    assert!(!shapes.editing() && !shapes.tapped(pt(310.0, 10.0)), "asked once");
}

#[test]
fn a_double_tap_on_a_text_edits_it_with_any_tool_but_the_pen() {
    let items = page();
    let (mut shapes, mut ink) = (Shapes::default(), Ink::default());
    assert!(!down(&mut shapes, &mut ink, Tool::Eraser, (310.0, 10.0), 0.0, &items));
    assert!(down(&mut shapes, &mut ink, Tool::Eraser, (312.0, 12.0), 0.3, &items));
    assert!(shapes.editing());
    shapes.end(&mut ink);

    assert!(!down(&mut shapes, &mut ink, Tool::Pen, (310.0, 10.0), 1.4, &items));
    assert!(!down(&mut shapes, &mut ink, Tool::Pen, (310.0, 10.0), 1.5, &items), "the pen draws");
    assert!(!down(&mut shapes, &mut ink, Tool::Eraser, (310.0, 10.0), 2.0, &items));
    assert!(!down(&mut shapes, &mut ink, Tool::Eraser, (310.0, 10.0), 2.5, &items), "too slow");
    assert!(!down(&mut shapes, &mut ink, Tool::Eraser, (330.0, 10.0), 2.6, &items), "too far");
    assert!(!down(&mut shapes, &mut ink, Tool::Lasso, (150.0, 150.0), 3.0, &items));
    assert!(
        !down(&mut shapes, &mut ink, Tool::Lasso, (150.0, 150.0), 3.1, &items),
        "no text there"
    );
    assert!(!shapes.editing());
}

#[test]
fn a_double_tap_on_another_text_ends_the_one_being_typed_and_edits_it() {
    let items = page();
    let (mut shapes, mut ink) = (Shapes::default(), Ink::default());
    assert!(down(&mut shapes, &mut ink, Tool::Text, (150.0, 150.0), 4.0, &items));
    shapes.typed("new", &mut ink);
    assert!(
        !down(&mut shapes, &mut ink, Tool::Lasso, (310.0, 10.0), 5.0, &items),
        "it ends the text and is the lasso's"
    );
    assert!(!down(&mut shapes, &mut ink, Tool::Lasso, (310.0, 10.0), 5.2, &items));
    assert!(shapes.tapped(pt(310.0, 10.0)) && shapes.editing());
    assert_eq!(changes(&mut ink).len(), 1, "the new text went");
}

#[test]
fn a_box_is_a_drag_from_corner_to_corner_either_way() {
    let (mut shapes, mut ink) = (Shapes::default(), Ink::default());
    assert!(down(&mut shapes, &mut ink, Tool::Box, (50.0, 60.0), 0.0, &[]));
    assert!(shapes.drag(pt(10.04, 20.0)));
    let showing = shapes.shown(HERE, Vec::new(), 1.0, &THEME);
    assert!(matches!(&showing[..], [i] if i.colour == RED && matches!(i.shape, Shape::Box { .. })));
    assert!(shapes.up(None, &[], &mut ink));
    let frame = frame(10.0, 20.0, 40.0, 40.0);
    assert_eq!(changes(&mut ink), vec![put(Shape::Box { frame, width: LINE_WIDTH })]);
    assert!(shapes.shown(HERE, Vec::new(), 1.0, &THEME).is_empty(), "the lift took it off");
    assert!(!shapes.up(None, &[], &mut ink), "nothing under the pen");

    // A tap, or a drag shorter than the least, makes nothing.
    drag(&mut shapes, &mut ink, Tool::Box, (0.0, 0.0), (2.0, 2.0), &[]);
    assert!(changes(&mut ink).is_empty(), "too short");
    let zoomed = Press { zoom: 4.0, ..press(0.0, 0.0, 9.0) };
    assert!(shapes.down((Tool::Box, RED), HERE, zoomed, &[], &mut ink));
    assert!(shapes.up(Some(pt(2.0, 0.0)), &[], &mut ink));
    assert_eq!(changes(&mut ink).len(), 1, "two units at four times is eight points");
}

#[test]
fn an_arrow_end_that_lands_on_an_item_binds_to_it() {
    let items = page();
    let (mut shapes, mut ink) = (Shapes::default(), Ink::default());
    drag(&mut shapes, &mut ink, Tool::Arrow, (50.0, 50.0), (305.0, -5.0), &items);
    let bound = [Some(ItemId(1)), Some(ItemId(2))];
    let arrow =
        Shape::Arrow { from: pt(50.0, 50.0), to: pt(305.0, -5.0), width: LINE_WIDTH, bound };
    assert_eq!(changes(&mut ink), vec![put(arrow)]);

    // Empty space, an arrow and local ink bind to nothing.
    drag(&mut shapes, &mut ink, Tool::Arrow, (200.0, 199.0), (520.0, 20.0), &items);
    let [Change::Put { shape: Shape::Arrow { bound, .. }, .. }] = &changes(&mut ink)[..] else {
        panic!()
    };
    assert_eq!(*bound, [None, None]);
}

#[test]
fn an_arrow_bound_to_a_moved_item_follows_it() {
    let mut items = page();
    let bound = [Some(ItemId(1)), None];
    items.push(item(
        4,
        Shape::Arrow { from: pt(50.0, 50.0), to: pt(200.0, 50.0), width: 2.0, bound },
    ));
    let both = [Some(ItemId(1)), Some(ItemId(2))];
    items.push(item(
        5,
        Shape::Arrow { from: pt(50.0, 60.0), to: pt(310.0, 10.0), width: 2.0, bound: both },
    ));
    let by = pt(10.0, -5.0);
    let follows = follow(&shown(&items), &[ItemId(1)], by);
    let moved = |id, from, to, bound| item(id, Shape::Arrow { from, to, width: 2.0, bound });
    assert_eq!(
        follows,
        vec![
            moved(4, pt(60.0, 45.0), pt(200.0, 50.0), bound),
            moved(5, pt(60.0, 55.0), pt(310.0, 10.0), both)
        ],
        "only the bound end moves"
    );
    assert!(
        follow(&shown(&items), &[ItemId(1), ItemId(4), ItemId(5)], by).is_empty(),
        "moved with it"
    );
    assert!(follow(&shown(&items), &[ItemId(3)], by).is_empty(), "nothing bound to the arrow");

    // The lasso's move of the box takes the arrow along, in one change.
    let (mut select, mut ink) = (Select::default(), Ink::default());
    let view = shown(&items);
    let lasso = [(-10.0, -10.0), (110.0, -10.0), (110.0, 110.0), (-10.0, 110.0)];
    assert!(select.down(Tool::Lasso, HERE, pt(-10.0, -10.0), 1.0, &view));
    for &(x, y) in &lasso[1..] {
        select.drag(pt(x, y), &view);
    }
    select.up(None, &view, &mut ink);
    assert!(select.down(Tool::Lasso, HERE, pt(50.0, 50.0), 1.0, &view));
    let dragging = select.shown(HERE, shown(&items), 1.0, &THEME);
    select.drag(pt(60.0, 45.0), &view);
    let dragging2 = select.shown(HERE, shown(&items), 1.0, &THEME);
    assert_ne!(dragging, dragging2);
    let arrow = dragging2.iter().find(|i| i.id == ItemId(4)).unwrap();
    assert!(
        matches!(arrow.shape, Shape::Arrow { from, .. } if from == pt(60.0, 45.0)),
        "it follows as it drags"
    );
    select.up(None, &view, &mut ink);
    let [Change::Move { ids, follow: follows, .. }] = &changes(&mut ink)[..] else { panic!() };
    assert_eq!(ids, &vec![ItemId(1)]);
    assert_eq!(follows.iter().map(|i| i.id.0).collect::<Vec<_>>(), [4, 5]);
}

#[test]
fn a_text_being_typed_shows_its_caret_frame_and_hint_sized_for_the_screen() {
    let (mut shapes, mut ink) = (Shapes::default(), Ink::default());
    assert!(down(&mut shapes, &mut ink, Tool::Text, (10.0, 214.4), 0.0, &[]));
    shapes.typed("Hi", &mut ink);
    let showing = shapes.shown(HERE, Vec::new(), 2.0, &THEME);
    let ids: Vec<ItemId> = showing.iter().map(|i| i.id).collect();
    assert_eq!(ids, [EDITING, CARET_ID, FRAME_ID, HINT_ID], "the chrome draws over the text");
    let text = glyph(10.0, 200.0, "Hi");
    let caret = Shape::Caret { frame: text, text: "Hi".into(), size: TEXT_SIZE, width: 1.0 };
    assert_eq!((showing[1].colour, &showing[1].shape), (THEME.focus, &caret), "2 points at zoom 2");
    let around = text.outset(2.0);
    let dashed = Shape::Panel { frame: around, fill: None, width: 0.5, dashed: true };
    assert_eq!((showing[2].colour, &showing[2].shape), (THEME.muted, &dashed));
    let Shape::Text { frame: under, text, size } = &showing[3].shape else { panic!() };
    assert_eq!((text.as_str(), *size), ("esc or a tap outside ends the text", 5.5));
    assert_eq!(showing[3].colour, THEME.muted);
    assert_eq!((under.x, under.y), (around.x, around.y + around.height + 2.0), "under the frame");

    // At zoom 2 the canvas draws them at screen size: the caret is 2 points wide.
    let items = showing.iter().map(AsRef::as_ref);
    let view = crate::layout::Viewport::default().zoom_about(pt(0.0, 0.0), 2.0);
    let screen = crate::layout::Size { width: 800.0, height: 600.0 };
    let drawn = crate::scene::canvas::build(items, view, screen, &THEME, None);
    let marks: Vec<&Mark> = drawn.draws.iter().map(|d| &d.mark).collect();
    let two = |w: f64| (w - 2.0).abs() < 1e-9;
    assert!(marks.iter().any(|m| matches!(m, Mark::Caret { width, .. } if two(*width))));
    assert!(marks.iter().any(|m| matches!(m, Mark::Panel { dashed: true, .. })));

    shapes.typed("\u{1b}", &mut ink);
    assert!(shapes.shown(HERE, Vec::new(), 2.0, &THEME).is_empty(), "ending takes it all off");
}
