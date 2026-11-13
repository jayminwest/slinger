use super::*;

const SCREEN: Size = Size { width: 800.0, height: 600.0 };
const INK: Rgb = Rgb { r: 200, g: 40, b: 40 };

fn theme() -> Theme {
    Theme::default()
}

fn pt(x: f64, y: f64) -> Point {
    Point { x, y }
}

fn rect(x: f64, y: f64, width: f64, height: f64) -> Rect {
    Rect { x, y, width, height }
}

fn item(id: u64, shape: Shape) -> Item {
    Item { id: ItemId(id), colour: INK, shape }
}

fn stroke(id: u64, points: &[(f64, f64)]) -> Item {
    let points = points.iter().map(|&(x, y)| pt(x, y)).collect();
    item(id, Shape::Stroke { points, width: 2.0 })
}

fn at(items: &[Item], view: Viewport, last: Option<&Scene>) -> Scene {
    build(items, view, SCREEN, &theme(), last)
}

fn ids(scene: &Scene) -> Vec<u64> {
    scene.draws.iter().map(|d| d.id.0).collect()
}

fn near(a: Point, b: Point) -> bool {
    (a.x - b.x).abs() < 1e-9 && (a.y - b.y).abs() < 1e-9
}

fn page() -> Vec<Item> {
    vec![
        stroke(1, &[(10.0, 10.0), (50.0, 40.0)]),
        stroke(2, &[(5000.0, 5000.0), (5010.0, 5010.0)]),
        item(
            3,
            Shape::Text {
                frame: rect(100.0, 100.0, 80.0, 20.0),
                text: "hi".into(),
                size: 16.0,
                column: None,
            },
        ),
        item(4, Shape::Box { frame: rect(-900.0, 0.0, 50.0, 50.0), width: 2.0 }),
        item(
            5,
            Shape::Arrow {
                from: pt(200.0, 300.0),
                to: pt(300.0, 300.0),
                width: 2.0,
                bound: [None; 2],
            },
        ),
    ]
}

#[test]
fn only_visible_items_are_in_the_draw_list() {
    let scene = at(&page(), Viewport::default(), None);
    assert_eq!(ids(&scene), [1, 3, 5]);
}

#[test]
fn panning_and_zooming_out_bring_other_items_into_view() {
    let view = Viewport::default().pan(1000.0, 0.0);
    assert_eq!(ids(&at(&page(), view, None)), [4]);
    let out = Viewport::default().zoom_about(pt(0.0, 0.0), 0.1);
    assert_eq!(ids(&at(&page(), out, None)), [1, 2, 3, 5]);
}

#[test]
fn an_item_partly_on_screen_is_drawn() {
    let items = [stroke(1, &[(-100.0, 10.0), (5.0, 10.0)])];
    assert_eq!(ids(&at(&items, Viewport::default(), None)), [1]);
}

#[test]
fn a_stroke_with_no_points_draws_nothing() {
    let items = [stroke(1, &[])];
    assert_eq!(at(&items, Viewport::default(), None).draws, []);
}

#[test]
fn marks_are_in_screen_points_and_scale_with_the_zoom() {
    let view = Viewport::default().zoom_about(pt(0.0, 0.0), 2.0).pan(10.0, 0.0);
    let scene = at(&page(), view, None);
    // Smoothed in canvas points, then placed.
    let curves = smooth::smooth(&[pt(10.0, 10.0), pt(50.0, 40.0)], 2.0);
    let curves = curves.iter().map(|c| c.map(|p| view.to_screen(p))).collect();
    assert_eq!(scene.draws[0].mark, Mark::Stroke { start: pt(30.0, 20.0), curves, width: 4.0 });
    assert_eq!(scene.draws[0].bounds, rect(28.0, 18.0, 84.0, 64.0));
    assert_eq!(
        scene.draws[1].mark,
        Mark::Text {
            at: pt(210.0, 200.0),
            width: 80.0,
            text: "hi".into(),
            size: 16.0,
            line: 0.0,
            zoom: 2.0
        }
    );
    let boxed = [item(9, Shape::Box { frame: rect(1.0, 2.0, 3.0, 4.0), width: 1.0 })];
    let scene = at(&boxed, view, None);
    assert_eq!(scene.draws[0].mark, Mark::Box { frame: rect(12.0, 4.0, 6.0, 8.0), width: 2.0 });
    assert_eq!(scene.draws[0].bounds, rect(11.0, 3.0, 8.0, 10.0));
}

#[test]
fn a_dot_has_the_bounds_of_its_width() {
    let scene = at(&[stroke(1, &[(4.0, 5.0)])], Viewport::default(), None);
    assert_eq!(scene.draws[0].bounds, rect(3.0, 4.0, 2.0, 2.0));
}

#[test]
fn ink_keeps_its_colour_unless_the_background_would_hide_it() {
    let dark = theme();
    assert_eq!(legible(INK, &dark), INK);
    assert_eq!(legible(Rgb { r: 30, g: 30, b: 30 }, &dark), dark.text);
    assert_eq!(legible(Rgb { r: 60, g: 100, b: 220 }, &dark), Rgb { r: 60, g: 100, b: 220 });
    let light = Theme { background: Rgb { r: 255, g: 255, b: 255 }, ..dark };
    assert_eq!(legible(Rgb { r: 30, g: 30, b: 30 }, &light), Rgb { r: 30, g: 30, b: 30 });
    let mut items = page();
    items[0].colour = dark.background;
    let scene = at(&items, Viewport::default(), None);
    assert_eq!((scene.draws[0].colour, scene.draws[1].colour), (dark.text, INK));
}

#[test]
fn an_arrow_head_points_back_along_the_shaft() {
    let scene = at(&page(), Viewport::default(), None);
    let Mark::Arrow { from, to, barbs, width } = scene.draws[2].mark else {
        panic!("not an arrow: {:?}", scene.draws[2].mark);
    };
    assert_eq!((from, to, width), (pt(200.0, 300.0), pt(300.0, 300.0), 2.0));
    let (dx, dy) = (HEAD_LENGTH * HEAD_ANGLE.cos(), HEAD_LENGTH * HEAD_ANGLE.sin());
    assert!(near(barbs[0], pt(300.0 - dx, 300.0 + dy)));
    assert!(near(barbs[1], pt(300.0 - dx, 300.0 - dy)));
    assert!(scene.draws[2].bounds.intersection(rect(barbs[0].x, barbs[0].y, 0.0, 0.0)).is_some());
}

#[test]
fn an_arrow_of_no_length_still_has_a_head() {
    let b = barbs(pt(5.0, 5.0), pt(5.0, 5.0));
    assert!(b[0].x < 5.0 && b[1].x < 5.0);
}

#[test]
fn the_first_scene_changes_the_whole_pane() {
    let scene = at(&page(), Viewport::default(), None);
    assert_eq!(scene.changed, Some(rect(0.0, 0.0, 800.0, 600.0)));
    assert_eq!(scene.background, theme().background);
}

#[test]
fn the_same_items_change_nothing() {
    let first = at(&page(), Viewport::default(), None);
    let again = at(&page(), Viewport::default(), Some(&first));
    assert_eq!(again.changed, None);
    assert_eq!(again.draws, first.draws);
}

#[test]
fn an_added_item_changes_its_box() {
    let first = at(&page(), Viewport::default(), None);
    let mut items = page();
    items.push(stroke(6, &[(400.0, 400.0), (420.0, 410.0)]));
    let next = at(&items, Viewport::default(), Some(&first));
    assert_eq!(next.changed, Some(rect(399.0, 399.0, 22.0, 12.0)));
}

#[test]
fn a_removed_item_changes_the_box_it_left() {
    let first = at(&page(), Viewport::default(), None);
    let items: Vec<Item> = page().into_iter().filter(|i| i.id != ItemId(1)).collect();
    let next = at(&items, Viewport::default(), Some(&first));
    assert_eq!(next.changed, Some(rect(9.0, 9.0, 42.0, 32.0)));
}

#[test]
fn a_moved_or_recoloured_item_changes_where_it_was_and_where_it_is() {
    let first = at(&page(), Viewport::default(), None);
    let mut items = page();
    items[0] = stroke(1, &[(110.0, 10.0), (150.0, 40.0)]);
    let next = at(&items, Viewport::default(), Some(&first));
    assert_eq!(next.changed, Some(rect(9.0, 9.0, 142.0, 32.0)));
    let mut items = page();
    items[0].colour = Rgb { r: 1, g: 200, b: 1 };
    let next = at(&items, Viewport::default(), Some(&first));
    assert_eq!(next.changed, Some(rect(9.0, 9.0, 42.0, 32.0)));
}

#[test]
fn a_change_off_the_screen_changes_nothing_on_it() {
    let first = at(&page(), Viewport::default(), None);
    let mut items = page();
    items[1] = stroke(2, &[(6000.0, 6000.0)]);
    assert_eq!(at(&items, Viewport::default(), Some(&first)).changed, None);
}

#[test]
fn a_change_partly_off_the_screen_is_clipped_to_it() {
    let first = at(&[], Viewport::default(), None);
    let items = [stroke(1, &[(-100.0, 10.0), (5.0, 10.0)])];
    let next = at(&items, Viewport::default(), Some(&first));
    assert_eq!(next.changed, Some(rect(0.0, 9.0, 6.0, 2.0)));
}

#[test]
fn a_new_viewport_pane_size_or_background_changes_the_whole_pane() {
    let first = at(&page(), Viewport::default(), None);
    let panned = at(&page(), Viewport::default().pan(1.0, 0.0), Some(&first));
    assert_eq!(panned.changed, Some(rect(0.0, 0.0, 800.0, 600.0)));
    let small = Size { width: 400.0, height: 300.0 };
    let resized = build(&page(), Viewport::default(), small, &theme(), Some(&first));
    assert_eq!(resized.changed, Some(rect(0.0, 0.0, 400.0, 300.0)));
    let light = Theme { background: Rgb { r: 250, g: 250, b: 250 }, ..theme() };
    let repainted = build(&page(), Viewport::default(), SCREEN, &light, Some(&first));
    assert_eq!(repainted.changed, Some(rect(0.0, 0.0, 800.0, 600.0)));
}

#[test]
fn a_pen_line_that_changes_at_its_end_repaints_only_its_end() {
    let line = |n: u32| {
        let points: Vec<(f64, f64)> = (1..=n).map(|i| (f64::from(i) * 10.0, 100.0)).collect();
        [stroke(1, &points)]
    };
    let first = at(&line(10), Viewport::default(), None);
    let next = at(&line(11), Viewport::default(), Some(&first));
    // Smoothing moves the last few curves, not the line before them.
    assert_eq!(next.changed, Some(rect(88.0, 98.0, 24.0, 4.0)));
    let dot = at(&line(1), Viewport::default(), None);
    let two = at(&line(2), Viewport::default(), Some(&dot));
    assert_eq!(two.changed, Some(rect(8.0, 98.0, 14.0, 4.0)));
    let shorter = at(&line(9), Viewport::default(), Some(&first));
    assert_eq!(shorter.changed, Some(rect(78.0, 98.0, 24.0, 4.0)), "shorter: only its end");
}

#[test]
fn paint_stops_at_the_pane_edge_when_ink_runs_past_it() {
    // A pan right puts a text and a box part past the pane's left edge
    // (slinger-d90e4d): both are drawn, and their ink runs past the edge.
    let text = Shape::Text {
        frame: rect(20.0, 100.0, 80.0, 20.0),
        text: "hi".into(),
        size: 16.0,
        column: None,
    };
    let boxed = Shape::Box { frame: rect(30.0, 200.0, 40.0, 40.0), width: 2.0 };
    let scene = at(&[item(1, text), item(2, boxed)], Viewport::default().pan(-60.0, 0.0), None);
    assert_eq!(ids(&scene), [1, 2]);
    assert!(scene.draws.iter().all(|d| d.bounds.x < 0.0));
    // AppKit may ask for more than the pane: only the pane is painted.
    let area = scene.clip(rect(-200.0, -50.0, 1200.0, 700.0));
    assert_eq!(area, Some(SCREEN.rect()));
    assert_eq!(scene.touching(SCREEN.rect()).count(), 2);
    // Nothing wholly beside the pane is painted.
    assert_eq!(scene.clip(rect(-150.0, 0.0, 100.0, 600.0)), None);
    // A box part in the pane is cut to it, with only the draws there.
    let edge = scene.clip(rect(-100.0, 90.0, 110.0, 40.0));
    assert_eq!(edge, Some(rect(0.0, 90.0, 10.0, 40.0)));
    let there: Vec<u64> =
        edge.into_iter().flat_map(|e| scene.touching(e)).map(|d| d.id.0).collect();
    assert_eq!(there, [1]);
}

#[test]
fn chrome_scales_with_the_zoom_and_moves_like_an_item() {
    let tint = Some(Fill { colour: INK, alpha: 0.07 });
    let panel =
        Shape::Panel { frame: rect(10.0, 10.0, 20.0, 10.0), fill: tint, width: 1.0, dashed: true };
    let caret = Shape::Caret {
        frame: rect(10.0, 30.0, 48.0, 31.2),
        text: "Hi".into(),
        size: 24.0,
        width: 1.0,
    };
    assert_eq!(panel.bounds(), Some(rect(9.5, 9.5, 21.0, 11.0)), "half the line outside");
    assert_eq!(caret.bounds(), Some(rect(9.0, 29.0, 50.0, 33.2)), "a bar past the last character");
    let by = pt(5.0, -5.0);
    assert_eq!(panel.moved(by).bounds(), Some(rect(14.5, 4.5, 21.0, 11.0)));
    assert_eq!(caret.moved(by).bounds(), Some(rect(14.0, 24.0, 50.0, 33.2)));

    let view = Viewport::default().zoom_about(pt(0.0, 0.0), 2.0);
    let scene = at(&[item(1, panel), item(2, caret)], view, None);
    let Mark::Panel { frame, fill, width, dashed } = scene.draws[0].mark else { panic!() };
    assert_eq!((frame, fill, width, dashed), (rect(20.0, 20.0, 40.0, 20.0), tint, 2.0, true));
    let Mark::Caret { at, column, ref text, size, line, zoom, width } = scene.draws[1].mark else {
        panic!()
    };
    // Laid out as the canvas text it is in: in its rows, a size apart.
    assert_eq!(
        (at, column, text.as_str(), size, line, zoom, width),
        (pt(20.0, 60.0), glyphs::UNWRAPPED, "Hi", 24.0 * glyphs::FONT, 24.0, 2.0, 2.0)
    );
}

/// slinger-f66a97: a canvas text draws in the core's rows for its stored
/// column, a size apart, in the system font at a part of the size.
#[test]
fn a_canvas_text_draws_in_the_core_s_rows_a_size_apart() {
    let frame = rect(0.0, 0.0, 30.0, 48.0);
    let text = Shape::Text { frame, text: "ab cd".into(), size: 24.0, column: Some(48.0) };
    let scene = at(&[item(1, text)], Viewport::default(), None);
    let Mark::Text { width, ref text, size, line, .. } = scene.draws[0].mark else { panic!() };
    assert_eq!((text.as_str(), line), ("ab\ncd", 24.0), "two cells a row");
    assert_eq!((width, size), (glyphs::UNWRAPPED, 24.0 * glyphs::FONT), "its own breaks only");
}
