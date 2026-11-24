//! A fill: the inside of a filled box or a closed stroke, drawn as a tint
//! under all ink (docs/design/figures.md, "Draw order"). Coverage is
//! taken by scan lines, several to a pixel row, so the edge is smooth.

use super::{Mask, Raster, corners};
use crate::canvas::{Item, Point, Rect, Shape};

/// Scan lines to a pixel row.
const SUB: usize = 4;

impl Raster {
    /// Lays an item's fill over what is already drawn, at its alpha capped
    /// by [`FILL_MAX_ALPHA`](crate::canvas::style::FILL_MAX_ALPHA). An item
    /// with no fill draws nothing.
    pub fn fill(&mut self, item: &Item) {
        let Some(colour) = item.style().tint() else { return };
        let outline = match item.shape() {
            Shape::Box { frame, .. } => corners(self, *frame),
            Shape::Stroke { points, .. } => points.iter().map(|&p| self.at(p)).collect(),
            _ => return,
        };
        let Some(bounds) = Rect::around(outline.iter().copied()) else { return };
        let Some(mut mask) = self.mask(bounds, 1.0) else { return };
        mask.polygon(&outline);
        self.paint(&mask, colour);
    }
}

impl Mask {
    /// Covers the inside of the polygon `outline` (closed from its last
    /// point to its first), by the even-odd rule.
    fn polygon(&mut self, outline: &[Point]) {
        let Some(&last) = outline.last() else { return };
        let edges: Vec<(Point, Point)> = std::iter::once(last)
            .chain(outline.iter().copied())
            .zip(outline.iter().copied())
            .collect();
        let mut xs = Vec::new();
        for row in 0..self.height {
            for sub in 0..SUB {
                let y = super::float(self.y0 + row) + (super::float(sub) + 0.5) / super::float(SUB);
                xs.clear();
                for &(a, b) in &edges {
                    if (a.y <= y) != (b.y <= y) {
                        xs.push(a.x + (y - a.y) * (b.x - a.x) / (b.y - a.y));
                    }
                }
                xs.sort_by(f64::total_cmp);
                for &[x0, x1] in xs.as_chunks::<2>().0 {
                    self.span(row, x0, x1, 1.0 / super::float(SUB));
                }
            }
        }
    }

    /// Adds `weight` times how much of each pixel of mask row `row` lies
    /// between `x0` and `x1`, in tile pixels.
    fn span(&mut self, row: usize, x0: f64, x1: f64, weight: f64) {
        for col in 0..self.width {
            let left = super::float(self.x0 + col);
            let part = (x1.min(left + 1.0) - x0.max(left)).max(0.0);
            if part > 0.0 {
                let cover = &mut self.cover[row * self.width + col];
                *cover = (*cover + part * weight).min(1.0);
            }
        }
    }
}
