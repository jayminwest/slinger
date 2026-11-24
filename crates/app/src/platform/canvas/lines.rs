//! The lines of a canvas pane's marks: a pen line, a box and an arrow, each
//! in its dash, an arrow's heads solid; and the fills of filled boxes and
//! closed pen lines, which [`style::order`] paints under all ink
//! (docs/design/figures.md, "Draw order").

use objc2_app_kit::{NSBezierPath, NSLineCapStyle, NSLineJoinStyle, NSWindingRule};

use super::{color, line, ns, ns_rect};
use crate::layout::Point;
use crate::scene::canvas::style::{self, Dash};
use crate::scene::canvas::{Fill, Mark};

/// Strokes a pen line, a box or an arrow in the colour set; false for any
/// other mark, which this does not draw.
pub(super) fn ink(mark: &Mark) -> bool {
    match mark {
        Mark::Stroke { start, curves, width, dash, .. } => {
            let path = line(*width);
            curve(&path, *start, curves);
            dashed(&path, *dash, *width);
            path.stroke();
        }
        Mark::Box { frame, width, dash, .. } => {
            let path = NSBezierPath::bezierPathWithRect(ns_rect(*frame));
            path.setLineWidth(*width);
            if *dash != Dash::Solid {
                // Round ends, as the core's raster draws each dash and dot.
                path.setLineCapStyle(NSLineCapStyle::Round);
                path.setLineJoinStyle(NSLineJoinStyle::Round);
            }
            dashed(&path, *dash, *width);
            path.stroke();
        }
        Mark::Arrow { from, to, heads, width, dash } => {
            let shaft = line(*width);
            shaft.moveToPoint(ns(*from));
            shaft.lineToPoint(ns(*to));
            dashed(&shaft, *dash, *width);
            shaft.stroke();
            let path = line(*width);
            for (tip, barbs) in heads {
                for barb in barbs {
                    path.moveToPoint(ns(*barb));
                    path.lineToPoint(ns(*tip));
                }
            }
            path.stroke();
        }
        _ => return false,
    }
    true
}

/// Paints the fill of a filled box or a closed pen line: its inside, by the
/// even-odd rule as the core's raster fills it, at its capped alpha.
pub(super) fn fill(mark: &Mark) {
    let path = match mark {
        Mark::Box { frame, .. } => NSBezierPath::bezierPathWithRect(ns_rect(*frame)),
        Mark::Stroke { start, curves, .. } => {
            let path = NSBezierPath::new();
            curve(&path, *start, curves);
            path.closePath();
            path
        }
        _ => return,
    };
    let Some(Fill { colour, alpha }) = style::fill(mark) else { return };
    path.setWindingRule(NSWindingRule::EvenOdd);
    color(colour).colorWithAlphaComponent(alpha).setFill();
    path.fill();
}

/// `path` along the curves from `start`.
fn curve(path: &NSBezierPath, start: Point, curves: &[[Point; 3]]) {
    path.moveToPoint(ns(start));
    for [c1, c2, end] in curves {
        path.curveToPoint_controlPoint1_controlPoint2(ns(*end), ns(*c1), ns(*c2));
    }
}

/// Sets `path`'s dashes for `dash` on a line `width` wide; a solid line
/// has none.
fn dashed(path: &NSBezierPath, dash: Dash, width: f64) {
    if let Some(pattern) = style::pattern(dash, width) {
        let count = pattern.len().cast_signed();
        // SAFETY: the pattern is `count` floats that outlive the call.
        unsafe { path.setLineDash_count_phase(pattern.as_ptr(), count, 0.0) };
    }
}
