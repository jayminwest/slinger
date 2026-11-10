//! What a canvas pane shows. Pure: from a page's items and a [`Viewport`], the
//! marks to draw in screen points, and the screen box that changed since the
//! last scene, so `platform` repaints only that box. Items off the screen are
//! not in the draw list, so the cost of a frame follows what is visible, not
//! the size of the page.

use std::collections::HashMap;
use std::f64::consts::FRAC_PI_6;

use crate::layout::{Point, Rect, Size, Viewport};

/// An item's id, unique within its page. The id is how a scene knows an item
/// it drew last time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ItemId(pub u64);

/// One item on a page. Later items draw on top of earlier ones.
#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    pub id: ItemId,
    pub shape: Shape,
}

/// What an item is, in canvas points. Widths are line widths.
#[derive(Debug, Clone, PartialEq)]
pub enum Shape {
    /// A pen line through `points` at a constant width.
    Stroke { points: Vec<Point>, width: f64 },
    /// Text set inside `frame`, `size` points high.
    Text { frame: Rect, text: String, size: f64 },
    /// The outline of `frame`.
    Box { frame: Rect, width: f64 },
    /// A line from `from` to `to`, with its head at `to`.
    Arrow { from: Point, to: Point, width: f64 },
}

/// One thing to draw: a [`Shape`] placed by the viewport, in screen points.
#[derive(Debug, Clone, PartialEq)]
pub enum Mark {
    Stroke {
        points: Vec<Point>,
        width: f64,
    },
    Text {
        frame: Rect,
        text: String,
        size: f64,
    },
    Box {
        frame: Rect,
        width: f64,
    },
    /// The shaft from `from` to `to`, and the head: a line from `to` to each barb.
    Arrow {
        from: Point,
        to: Point,
        barbs: [Point; 2],
        width: f64,
    },
}

/// A mark, the item it draws, and the screen box its ink covers.
#[derive(Debug, Clone, PartialEq)]
pub struct Draw {
    pub id: ItemId,
    pub bounds: Rect,
    pub mark: Mark,
}

/// One canvas pane, fully decided.
#[derive(Debug, Clone, PartialEq)]
pub struct Scene {
    pub viewport: Viewport,
    /// The pane's size in screen points.
    pub screen: Size,
    /// What to draw, bottom first.
    pub draws: Vec<Draw>,
    /// The screen box to repaint since the last scene; `None` when nothing
    /// on the screen changed.
    pub changed: Option<Rect>,
}

/// The length of an arrow's head barbs, in canvas points.
const HEAD_LENGTH: f64 = 12.0;
/// The angle between an arrow's shaft and each barb.
const HEAD_ANGLE: f64 = FRAC_PI_6;

/// The scene for `items` seen through `viewport` on a pane of `screen` size.
/// Pass the scene before this one as `last` to learn what changed: with no
/// last scene, or after the viewport or the size changed, the whole pane did.
pub fn build(items: &[Item], viewport: Viewport, screen: Size, last: Option<&Scene>) -> Scene {
    let visible = viewport.visible(screen);
    let draws: Vec<Draw> = items
        .iter()
        .filter_map(|item| {
            let bounds = item.shape.bounds()?;
            bounds.intersection(visible)?;
            let bounds = viewport.rect_to_screen(bounds);
            Some(Draw { id: item.id, bounds, mark: item.shape.mark(viewport) })
        })
        .collect();
    let pane = Rect { x: 0.0, y: 0.0, width: screen.width, height: screen.height };
    let changed = match last {
        Some(last) if last.viewport == viewport && last.screen == screen => {
            changed_between(&last.draws, &draws).and_then(|r| r.intersection(pane))
        }
        _ => Some(pane),
    };
    Scene { viewport, screen, draws, changed }
}

/// The box around every draw that is in one list and not, unchanged, in the other.
fn changed_between(old: &[Draw], new: &[Draw]) -> Option<Rect> {
    let before: HashMap<ItemId, &Draw> = old.iter().map(|d| (d.id, d)).collect();
    let after: HashMap<ItemId, &Draw> = new.iter().map(|d| (d.id, d)).collect();
    let gone = old.iter().filter(|d| after.get(&d.id).copied() != Some(*d));
    let came = new.iter().filter(|d| before.get(&d.id).copied() != Some(*d));
    gone.chain(came).map(|d| d.bounds).reduce(Rect::union)
}

impl Shape {
    /// The canvas box the shape's ink covers; `None` when it has no ink.
    fn bounds(&self) -> Option<Rect> {
        match self {
            Self::Stroke { points, width } => {
                Rect::around(points.iter().copied()).map(|r| r.outset(width / 2.0))
            }
            Self::Text { frame, .. } => Some(*frame),
            Self::Box { frame, width } => Some(frame.outset(width / 2.0)),
            Self::Arrow { from, to, width } => {
                Rect::around([*from, *to]).map(|r| r.outset(width / 2.0 + HEAD_LENGTH))
            }
        }
    }

    /// The shape on the screen.
    fn mark(&self, view: Viewport) -> Mark {
        let zoom = view.zoom();
        match self {
            Self::Stroke { points, width } => Mark::Stroke {
                points: points.iter().map(|p| view.to_screen(*p)).collect(),
                width: width * zoom,
            },
            Self::Text { frame, text, size } => Mark::Text {
                frame: view.rect_to_screen(*frame),
                text: text.clone(),
                size: size * zoom,
            },
            Self::Box { frame, width } => {
                Mark::Box { frame: view.rect_to_screen(*frame), width: width * zoom }
            }
            Self::Arrow { from, to, width } => Mark::Arrow {
                from: view.to_screen(*from),
                to: view.to_screen(*to),
                barbs: barbs(*from, *to).map(|p| view.to_screen(p)),
                width: width * zoom,
            },
        }
    }
}

/// The ends of an arrow's two barbs, in canvas points. They point back along
/// the shaft; a shaft of no length points right, so its head points left.
fn barbs(from: Point, to: Point) -> [Point; 2] {
    let heading = (to.y - from.y).atan2(to.x - from.x);
    [heading - HEAD_ANGLE, heading + HEAD_ANGLE].map(|angle| Point {
        x: to.x - HEAD_LENGTH * angle.cos(),
        y: to.y - HEAD_LENGTH * angle.sin(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCREEN: Size = Size { width: 800.0, height: 600.0 };

    fn pt(x: f64, y: f64) -> Point {
        Point { x, y }
    }

    fn rect(x: f64, y: f64, width: f64, height: f64) -> Rect {
        Rect { x, y, width, height }
    }

    fn stroke(id: u64, points: &[(f64, f64)]) -> Item {
        let points = points.iter().map(|&(x, y)| pt(x, y)).collect();
        Item { id: ItemId(id), shape: Shape::Stroke { points, width: 2.0 } }
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
            Item {
                id: ItemId(3),
                shape: Shape::Text {
                    frame: rect(100.0, 100.0, 80.0, 20.0),
                    text: "hi".into(),
                    size: 16.0,
                },
            },
            Item {
                id: ItemId(4),
                shape: Shape::Box { frame: rect(-900.0, 0.0, 50.0, 50.0), width: 2.0 },
            },
            Item {
                id: ItemId(5),
                shape: Shape::Arrow { from: pt(200.0, 300.0), to: pt(300.0, 300.0), width: 2.0 },
            },
        ]
    }

    #[test]
    fn only_visible_items_are_in_the_draw_list() {
        let scene = build(&page(), Viewport::default(), SCREEN, None);
        assert_eq!(ids(&scene), [1, 3, 5]);
    }

    #[test]
    fn panning_and_zooming_out_bring_other_items_into_view() {
        let view = Viewport::default().pan(1000.0, 0.0);
        assert_eq!(ids(&build(&page(), view, SCREEN, None)), [4]);
        let out = Viewport::default().zoom_about(pt(0.0, 0.0), 0.1);
        assert_eq!(ids(&build(&page(), out, SCREEN, None)), [1, 2, 3, 5]);
    }

    #[test]
    fn an_item_partly_on_screen_is_drawn() {
        let items = [stroke(1, &[(-100.0, 10.0), (5.0, 10.0)])];
        assert_eq!(ids(&build(&items, Viewport::default(), SCREEN, None)), [1]);
    }

    #[test]
    fn a_stroke_with_no_points_draws_nothing() {
        let items = [stroke(1, &[])];
        assert_eq!(build(&items, Viewport::default(), SCREEN, None).draws, []);
    }

    #[test]
    fn marks_are_in_screen_points_and_scale_with_the_zoom() {
        let view = Viewport::default().zoom_about(pt(0.0, 0.0), 2.0).pan(10.0, 0.0);
        let scene = build(&page(), view, SCREEN, None);
        assert_eq!(
            scene.draws[0].mark,
            Mark::Stroke { points: vec![pt(30.0, 20.0), pt(110.0, 80.0)], width: 4.0 }
        );
        assert_eq!(scene.draws[0].bounds, rect(28.0, 18.0, 84.0, 64.0));
        assert_eq!(
            scene.draws[1].mark,
            Mark::Text { frame: rect(210.0, 200.0, 160.0, 40.0), text: "hi".into(), size: 32.0 }
        );
        let boxed = [Item {
            id: ItemId(9),
            shape: Shape::Box { frame: rect(1.0, 2.0, 3.0, 4.0), width: 1.0 },
        }];
        let scene = build(&boxed, view, SCREEN, None);
        assert_eq!(scene.draws[0].mark, Mark::Box { frame: rect(12.0, 4.0, 6.0, 8.0), width: 2.0 });
        assert_eq!(scene.draws[0].bounds, rect(11.0, 3.0, 8.0, 10.0));
    }

    #[test]
    fn an_arrow_head_points_back_along_the_shaft() {
        let scene = build(&page(), Viewport::default(), SCREEN, None);
        let Mark::Arrow { from, to, barbs, width } = scene.draws[2].mark else {
            panic!("not an arrow: {:?}", scene.draws[2].mark);
        };
        assert_eq!((from, to, width), (pt(200.0, 300.0), pt(300.0, 300.0), 2.0));
        let (dx, dy) = (HEAD_LENGTH * HEAD_ANGLE.cos(), HEAD_LENGTH * HEAD_ANGLE.sin());
        assert!(near(barbs[0], pt(300.0 - dx, 300.0 + dy)));
        assert!(near(barbs[1], pt(300.0 - dx, 300.0 - dy)));
        assert!(
            scene.draws[2].bounds.intersection(rect(barbs[0].x, barbs[0].y, 0.0, 0.0)).is_some()
        );
    }

    #[test]
    fn an_arrow_of_no_length_still_has_a_head() {
        let b = barbs(pt(5.0, 5.0), pt(5.0, 5.0));
        assert!(b[0].x < 5.0 && b[1].x < 5.0);
    }

    #[test]
    fn the_first_scene_changes_the_whole_pane() {
        let scene = build(&page(), Viewport::default(), SCREEN, None);
        assert_eq!(scene.changed, Some(rect(0.0, 0.0, 800.0, 600.0)));
    }

    #[test]
    fn the_same_items_change_nothing() {
        let first = build(&page(), Viewport::default(), SCREEN, None);
        let again = build(&page(), Viewport::default(), SCREEN, Some(&first));
        assert_eq!(again.changed, None);
        assert_eq!(again.draws, first.draws);
    }

    #[test]
    fn an_added_item_changes_its_box() {
        let first = build(&page(), Viewport::default(), SCREEN, None);
        let mut items = page();
        items.push(stroke(6, &[(400.0, 400.0), (420.0, 410.0)]));
        let next = build(&items, Viewport::default(), SCREEN, Some(&first));
        assert_eq!(next.changed, Some(rect(399.0, 399.0, 22.0, 12.0)));
    }

    #[test]
    fn a_removed_item_changes_the_box_it_left() {
        let first = build(&page(), Viewport::default(), SCREEN, None);
        let items: Vec<Item> = page().into_iter().filter(|i| i.id != ItemId(1)).collect();
        let next = build(&items, Viewport::default(), SCREEN, Some(&first));
        assert_eq!(next.changed, Some(rect(9.0, 9.0, 42.0, 32.0)));
    }

    #[test]
    fn a_moved_item_changes_where_it_was_and_where_it_is() {
        let first = build(&page(), Viewport::default(), SCREEN, None);
        let mut items = page();
        items[0] = stroke(1, &[(110.0, 10.0), (150.0, 40.0)]);
        let next = build(&items, Viewport::default(), SCREEN, Some(&first));
        assert_eq!(next.changed, Some(rect(9.0, 9.0, 142.0, 32.0)));
    }

    #[test]
    fn a_change_off_the_screen_changes_nothing_on_it() {
        let first = build(&page(), Viewport::default(), SCREEN, None);
        let mut items = page();
        items[1] = stroke(2, &[(6000.0, 6000.0)]);
        assert_eq!(build(&items, Viewport::default(), SCREEN, Some(&first)).changed, None);
    }

    #[test]
    fn a_change_partly_off_the_screen_is_clipped_to_it() {
        let first = build(&[], Viewport::default(), SCREEN, None);
        let items = [stroke(1, &[(-100.0, 10.0), (5.0, 10.0)])];
        let next = build(&items, Viewport::default(), SCREEN, Some(&first));
        assert_eq!(next.changed, Some(rect(0.0, 9.0, 6.0, 2.0)));
    }

    #[test]
    fn a_new_viewport_or_pane_size_changes_the_whole_pane() {
        let first = build(&page(), Viewport::default(), SCREEN, None);
        let panned = build(&page(), Viewport::default().pan(1.0, 0.0), SCREEN, Some(&first));
        assert_eq!(panned.changed, Some(rect(0.0, 0.0, 800.0, 600.0)));
        let small = Size { width: 400.0, height: 300.0 };
        let resized = build(&page(), Viewport::default(), small, Some(&first));
        assert_eq!(resized.changed, Some(rect(0.0, 0.0, 400.0, 300.0)));
    }
}
