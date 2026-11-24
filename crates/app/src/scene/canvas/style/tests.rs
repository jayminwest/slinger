//! A style draws as the core's raster draws it: the fill capped and under
//! all ink, the agent colour the theme's, dashes by the line's width, an
//! arrow's heads at the ends it names.

use super::*;
use crate::layout::{Rect, Size, Viewport};
use crate::scene::canvas::{Item, ItemId, Shape, build};

const RED: Rgb = Rgb { r: 200, g: 40, b: 40 };

fn pt(x: f64, y: f64) -> Point {
    Point { x, y }
}

fn frame() -> Rect {
    Rect { x: 10.0, y: 10.0, width: 100.0, height: 60.0 }
}

fn item(id: u64, shape: Shape, style: Style) -> Item {
    Item { id: ItemId(id), colour: RED, shape, style }
}

fn draws(items: &[Item], theme: &Theme) -> Vec<Draw> {
    let screen = Size { width: 800.0, height: 600.0 };
    build(items, Viewport::default(), screen, theme, None).draws
}

#[test]
fn a_fill_is_capped_at_forty_percent_and_never_made_legible() {
    let theme = Theme::default();
    let dark = Rgb { r: 0x20, g: 0x20, b: 0x20 };
    let solid = Style { fill: Some((dark, 255)), ..Style::default() };
    let fill = solid.tint(&theme).unwrap();
    assert_eq!(fill.colour, dark, "a fill is drawn as stored, even near the paper");
    assert!((fill.alpha - f64::from(FILL_MAX_ALPHA) / 255.0).abs() < 1e-12);
    let faint = Style { fill: Some((dark, 0x33)), ..Style::default() };
    assert!((faint.tint(&theme).unwrap().alpha - 0.2).abs() < 1e-12);
}

#[test]
fn the_agent_colour_draws_in_the_themes_agent_key() {
    let theme = Theme { agent: Rgb { r: 1, g: 200, b: 3 }, ..Theme::default() };
    assert_eq!(ink(AGENT, &theme), theme.agent);
    assert_eq!(ink(RED, &theme), RED);
    let filled = Style { fill: Some((AGENT, 0x33)), ..Style::default() };
    assert_eq!(filled.tint(&theme).unwrap().colour, theme.agent);
    assert_eq!(Theme::default().agent, AGENT);
    let boxed = item(1, Shape::Box { frame: frame(), width: 2.0 }, filled);
    let mut agents = boxed.clone();
    agents.colour = AGENT;
    assert_eq!(draws(&[agents], &theme)[0].colour, theme.agent);
}

#[test]
fn dashes_are_four_widths_on_and_three_off_and_dots_every_three() {
    assert_eq!(pattern(Dash::Solid, 2.0), None);
    assert_eq!(pattern(Dash::Dashed, 2.0), Some([8.0, 6.0]));
    assert_eq!(pattern(Dash::Dotted, 2.0), Some([0.0, 6.0]));
}

#[test]
fn an_arrow_has_the_heads_its_style_names() {
    let (from, to) = (pt(0.0, 0.0), pt(100.0, 0.0));
    assert_eq!(heads(from, to, Heads::None), vec![]);
    let end = heads(from, to, Heads::End);
    assert_eq!(end, vec![(to, barbs(from, to))]);
    let both = heads(from, to, Heads::Both);
    assert_eq!(both, vec![(to, barbs(from, to)), (from, barbs(to, from))]);
    assert!(both[1].1.iter().all(|b| b.x > 0.0), "the tail's head points back along the shaft");
}

#[test]
fn a_box_and_a_closed_stroke_carry_their_dash_and_fill() {
    let theme = Theme::default();
    let filled = Style { fill: Some((RED, 0xff)), dash: Dash::Dashed, ..Style::default() };
    let points = vec![pt(0.0, 0.0), pt(40.0, 0.0), pt(40.0, 40.0), pt(0.0, 40.0)];
    let loop_ = Style { closed: true, dash: Dash::Dotted, ..filled };
    let items = [
        item(1, Shape::Box { frame: frame(), width: 2.0 }, filled),
        item(2, Shape::Stroke { points, width: 2.0 }, loop_),
    ];
    let drawn = draws(&items, &theme);
    let Mark::Box { dash, fill, .. } = drawn[0].mark else { panic!("{:?}", drawn[0].mark) };
    assert_eq!((dash, fill), (Dash::Dashed, filled.tint(&theme)));
    let Mark::Stroke { start, ref curves, dash, fill, .. } = drawn[1].mark else { panic!() };
    assert_eq!((dash, fill), (Dash::Dotted, loop_.tint(&theme)));
    assert_eq!(curves.last().map(|c| c[2]), Some(start), "a closed line ends where it starts");
}

/// The order's draws as (pass, id).
fn painted(order: &[(Pass, &Draw)]) -> Vec<(Pass, u64)> {
    order.iter().map(|(pass, d)| (*pass, d.id.0)).collect()
}

#[test]
fn pdf_pages_then_fills_then_ink_and_a_filled_panel_starts_a_layer_over_them() {
    let theme = Theme::default();
    let filled = Style { fill: Some((RED, 0x33)), ..Style::default() };
    let pdf = Shape::Pdf {
        frame: frame(),
        hash: "h".into(),
        pdf_page: 1,
        page_size: (100.0, 60.0),
        crop: None,
        name: None,
    };
    let paper = Some(crate::scene::canvas::Fill { colour: theme.background, alpha: 1.0 });
    let items = [
        item(1, pdf, Style::default()),
        item(2, Shape::Box { frame: frame(), width: 2.0 }, Style::default()),
        item(3, Shape::Box { frame: frame(), width: 2.0 }, filled),
        item(
            4,
            Shape::Panel { frame: frame(), fill: paper, width: 1.0, dashed: false },
            Style::default(),
        ),
        item(5, Shape::Box { frame: frame(), width: 2.0 }, filled),
    ];
    let drawn = draws(&items, &theme);
    let order = order(&drawn);
    assert_eq!(
        painted(&order),
        vec![
            (Pass::Ink, 1),
            (Pass::Fill, 3),
            (Pass::Ink, 2),
            (Pass::Ink, 3),
            (Pass::Ink, 4),
            (Pass::Fill, 5),
            (Pass::Ink, 5)
        ],
        "a fill under all ink above the PDF; the panel's own fill is its ink"
    );
}
