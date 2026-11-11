//! What a canvas pane shows. Pure: from a page's items and a [`Viewport`], the
//! marks to draw in screen points, and the screen box that changed since the
//! last scene, so `platform` repaints only that box. Items off the screen are
//! not in the draw list, so the cost of a frame follows what is visible, not
//! the size of the page.
//!
//! Pen lines are smoothed here, not in `platform`: each corner of the line
//! through the pen's points is rounded by a curve through the middles of its
//! two sides, so the line keeps a constant width and no corners.

use std::collections::HashMap;
use std::f64::consts::FRAC_PI_6;

use crate::config::{Rgb, Theme};
use crate::layout::{Point, Rect, Size, Viewport};

/// An item's id, unique within its canvas: the number of the server's `i<n>`.
/// The id is how a scene knows an item it drew last time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ItemId(pub u64);

/// One item on a page. Later items draw on top of earlier ones.
#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    pub id: ItemId,
    pub colour: Rgb,
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
    /// A line from `start` through each cubic curve: two control points, then
    /// the curve's end. A line of one point is one curve of no length, which
    /// a round cap draws as a dot.
    Stroke {
        start: Point,
        curves: Vec<[Point; 3]>,
        width: f64,
    },
    /// `text` laid out `size` canvas points high in a column `width` canvas
    /// points wide, drawn from `at` at `zoom` times that size. The layout is
    /// in canvas points so a pan or a zoom never lays the text out again.
    Text {
        at: Point,
        width: f64,
        text: String,
        size: f64,
        zoom: f64,
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

/// A mark, the item it draws, its colour and the screen box its ink covers.
#[derive(Debug, Clone, PartialEq)]
pub struct Draw {
    pub id: ItemId,
    pub bounds: Rect,
    pub colour: Rgb,
    pub mark: Mark,
}

/// One canvas pane, fully decided.
#[derive(Debug, Clone, PartialEq)]
pub struct Scene {
    pub viewport: Viewport,
    /// The pane's size in screen points.
    pub screen: Size,
    /// The pane's paper, under every mark.
    pub background: Rgb,
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
/// The least contrast (WCAG ratio) ink keeps with the paper. Ink closer to
/// the background than this, such as black ink on a dark theme, draws in
/// the theme's text colour, so nothing on a page is invisible.
const MIN_CONTRAST: f64 = 1.5;

/// The scene for `items` seen through `viewport` on a pane of `screen` size,
/// on `theme`'s background. Pass the scene before this one as `last` to
/// learn what changed: with no last scene, or after the viewport, the size
/// or the background changed, the whole pane did.
pub fn build(
    items: &[Item],
    viewport: Viewport,
    screen: Size,
    theme: &Theme,
    last: Option<&Scene>,
) -> Scene {
    let visible = viewport.visible(screen);
    let draws: Vec<Draw> = items
        .iter()
        .filter_map(|item| {
            let bounds = item.shape.bounds()?;
            bounds.intersection(visible)?;
            Some(Draw {
                id: item.id,
                bounds: viewport.rect_to_screen(bounds),
                colour: legible(item.colour, theme),
                mark: item.shape.mark(viewport),
            })
        })
        .collect();
    let background = theme.background;
    let pane = screen.rect();
    let changed = match last {
        Some(last)
            if last.viewport == viewport
                && last.screen == screen
                && last.background == background =>
        {
            changed_between(&last.draws, &draws).and_then(|r| r.intersection(pane))
        }
        _ => Some(pane),
    };
    Scene { viewport, screen, background, draws, changed }
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
            Self::Stroke { points, width } => {
                let points: Vec<Point> = points.iter().map(|p| view.to_screen(*p)).collect();
                // A stroke has a point, or it has no bounds and is not drawn.
                let start = points.first().copied().unwrap_or(Point { x: 0.0, y: 0.0 });
                Mark::Stroke { start, curves: smooth(&points), width: width * zoom }
            }
            Self::Text { frame, text, size } => Mark::Text {
                at: view.to_screen(Point { x: frame.x, y: frame.y }),
                width: frame.width,
                text: text.clone(),
                size: *size,
                zoom,
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

/// The line through `points` as cubic curves from the first point: each
/// inner point is the control of a quadratic from the middle of the side
/// before it to the middle of the side after it, and the last side runs
/// straight to the last point. The line stays inside the points' box.
fn smooth(points: &[Point]) -> Vec<[Point; 3]> {
    let mid = |a: Point, b: Point| Point { x: f64::midpoint(a.x, b.x), y: f64::midpoint(a.y, b.y) };
    // A quadratic from `s` with control `c` to `e`, as the same cubic.
    let quad = |s: Point, c: Point, e: Point| {
        let toward = |from: Point| Point {
            x: from.x + (c.x - from.x) * 2.0 / 3.0,
            y: from.y + (c.y - from.y) * 2.0 / 3.0,
        };
        [toward(s), toward(e), e]
    };
    let (Some(&first), Some(&last)) = (points.first(), points.last()) else { return Vec::new() };
    let mut at = first;
    let mut curves = Vec::with_capacity(points.len());
    for pair in points.windows(2).skip(1) {
        let end = mid(pair[0], pair[1]);
        curves.push(quad(at, pair[0], end));
        at = end;
    }
    curves.push([at, last, last]);
    curves
}

/// `ink`, or the theme's text colour when `ink` would not show on its
/// background.
fn legible(ink: Rgb, theme: &Theme) -> Rgb {
    let (a, b) = (luminance(ink), luminance(theme.background));
    let contrast = (a.max(b) + 0.05) / (a.min(b) + 0.05);
    if contrast < MIN_CONTRAST { theme.text } else { ink }
}

/// The relative luminance of an sRGB colour, 0 (black) to 1 (white).
fn luminance(c: Rgb) -> f64 {
    let linear = |v: u8| (f64::from(v) / 255.0).powf(2.2);
    0.2126 * linear(c.r) + 0.7152 * linear(c.g) + 0.0722 * linear(c.b)
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
mod tests;
