use super::*;
use crate::layout::Size;
use crate::scene::canvas::{self, Draw, ItemId, Scene};

const SCREEN: Size = Size { width: 800.0, height: 600.0 };
/// The size the core gives a workspace's button, in canvas points.
const WIDTH: f64 = 160.0;
const HEIGHT: f64 = 40.0;
const FRAME: Rect = Rect { x: 100.0, y: 50.0, width: WIDTH, height: HEIGHT };

fn button(id: u64, status: Option<Status>) -> Item {
    let colour = Rgb { r: 0, g: 0, b: 0 };
    Item { id: ItemId(id), colour, shape: Shape::Button { frame: FRAME, status } }
}

fn scene(items: &[Item], view: Viewport, last: Option<&Scene>) -> Scene {
    canvas::build(items, view, SCREEN, &Theme::default(), last)
}

fn drawn(status: Option<Status>) -> Draw {
    scene(&[button(1, status)], Viewport::default(), None).draws.remove(0)
}

#[test]
fn each_status_has_its_mark_and_colour_and_no_status_a_muted_ring() {
    let theme = Theme::default();
    let states = [
        (None, "○", theme.muted),
        (Some(Status::Working), "●", theme.focus),
        (Some(Status::Blocked), "■", theme.error),
        (Some(Status::Done), "●", super::sidebar::colour(Some(Status::Done), &theme)),
    ];
    for (status, glyph, colour) in states {
        let draw = drawn(status);
        assert_eq!(draw.colour, colour, "{status:?}");
        let Mark::Button { frame, label, at, size, width, zoom } = draw.mark else {
            panic!("{status:?}: {:?}", draw.mark)
        };
        assert_eq!(label, format!("{glyph} send to agent"));
        assert_eq!((frame, draw.bounds), (FRAME, FRAME), "its whole box, at zoom 1");
        assert_eq!((at.x, at.y), (FRAME.x + 12.0, FRAME.y + 12.0), "inside, centred");
        assert_eq!((size, width, zoom), (16.0, 1.5, 1.0));
    }
}

#[test]
fn it_scales_with_the_canvas() {
    let view = Viewport::default().zoom_about(Point { x: 0.0, y: 0.0 }, 2.0);
    let draw = scene(&[button(1, None)], view, None).draws.remove(0);
    let Mark::Button { frame, width, zoom, .. } = draw.mark else { panic!("{:?}", draw.mark) };
    assert_eq!((frame.width, frame.height), (2.0 * WIDTH, 2.0 * HEIGHT));
    assert_eq!((width, zoom), (3.0, 2.0));
}

#[test]
fn a_new_status_repaints_the_button_and_the_same_one_nothing() {
    let working = scene(&[button(1, Some(Status::Working))], Viewport::default(), None);
    let same = scene(&[button(1, Some(Status::Working))], Viewport::default(), Some(&working));
    assert_eq!(same.changed, None);
    let done = scene(&[button(1, Some(Status::Done))], Viewport::default(), Some(&working));
    assert_eq!(done.changed, Some(FRAME));
}

#[test]
fn shown_gives_each_button_the_status_and_leaves_the_rest() {
    let text = Item {
        id: ItemId(2),
        colour: Rgb { r: 9, g: 9, b: 9 },
        shape: Shape::Text { frame: FRAME, text: "hi".into(), size: 16.0, column: None },
    };
    let (a, b) = (button(1, None), text.clone());
    let items = shown(vec![Cow::Borrowed(&a), Cow::Borrowed(&b)], Some(Status::Blocked));
    assert_eq!(items[0].shape, Shape::Button { frame: FRAME, status: Some(Status::Blocked) });
    assert!(matches!(items[1], Cow::Borrowed(t) if *t == text), "untouched, not copied");
    let again = shown(vec![Cow::Borrowed(&a)], None);
    assert!(matches!(again[0], Cow::Borrowed(_)), "already so: not copied");
}

#[test]
fn a_button_is_found_under_a_point() {
    let items = [Cow::Owned(button(1, None))];
    assert_eq!(under(&items, Point { x: 101.0, y: 51.0 }), Some(FRAME));
    assert_eq!(under(&items, Point { x: 99.0, y: 51.0 }), None);
    assert_eq!(first(&items), Some(FRAME));
    assert_eq!(first(&[]), None);
}
