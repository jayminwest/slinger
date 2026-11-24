//! How an item is drawn past its shape (docs/design/figures.md, "The
//! model"): a fill, a dash, an arrow's heads and a stroke closed into a
//! loop, as the mirror reads them from the canvas file (`mirror::style`).
//! The rules here match the core's raster (`slinger_core::canvas::style`,
//! `render/raster`), so the window and a crop's PNG agree: the same fill cap,
//! the same dash geometry and one paint order.

use std::f64::consts::FRAC_PI_6;

use super::{Draw, Fill, Mark};
use crate::config::{Rgb, Theme};
use crate::layout::Point;

/// The agent colour as the canvas file stores it (the core's
/// `canvas::style::AGENT`): drawn in the theme's `agent` colour.
pub const AGENT: Rgb = Rgb { r: 0x8a, g: 0x5c, b: 0xf5 };

/// The most a fill covers what lies under it (40%), as the core's
/// `FILL_MAX_ALPHA`: ink over a fill always reads.
pub const FILL_MAX_ALPHA: u8 = 0x66;

/// The length of an arrow's head barbs, in canvas points.
pub(super) const HEAD_LENGTH: f64 = 12.0;
/// The angle between an arrow's shaft and each barb.
pub(super) const HEAD_ANGLE: f64 = FRAC_PI_6;

/// A dash is this many line widths long and its gap this many; a dot is
/// as round as the line is wide, every [`DOT_EVERY`] widths (the core's
/// `render/raster/dash.rs`).
const DASH_ON: f64 = 4.0;
const DASH_OFF: f64 = 3.0;
const DOT_EVERY: f64 = 3.0;

/// The least contrast (WCAG ratio) ink keeps with the paper. Ink closer to
/// the background than this, such as black ink on a dark theme, draws in
/// the theme's text colour, so nothing on a page is invisible.
const MIN_CONTRAST: f64 = 1.5;

/// How a line is drawn: whole, in dashes or in dots.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Dash {
    #[default]
    Solid,
    Dashed,
    Dotted,
}

/// Which ends of an arrow have a head.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Heads {
    /// At `to` only: the arrow as it always was.
    #[default]
    End,
    None,
    Both,
}

/// An item's style. The default is how every item drew before styles: no
/// fill, solid, a head at an arrow's `to`, a stroke open.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Style {
    /// The inside's colour and its alpha as stored: a box's, or a closed
    /// stroke's. Drawn under all ink, at most [`FILL_MAX_ALPHA`].
    pub fill: Option<(Rgb, u8)>,
    pub dash: Dash,
    pub heads: Heads,
    /// A stroke whose last point joins its first.
    pub closed: bool,
}

impl Style {
    /// The fill as drawn on `theme`: its colour (the agent colour as the
    /// theme's), never made legible (a fill is a tint), at its alpha capped
    /// at [`FILL_MAX_ALPHA`].
    pub fn tint(self, theme: &Theme) -> Option<Fill> {
        let (colour, alpha) = self.fill?;
        let alpha = f64::from(alpha.min(FILL_MAX_ALPHA)) / 255.0;
        Some(Fill { colour: agent(colour, theme), alpha })
    }
}

/// The colour ink stored as `colour` draws in on `theme`: the agent colour
/// as the theme's `agent`, then made legible on its background.
pub fn ink(colour: Rgb, theme: &Theme) -> Rgb {
    legible(agent(colour, theme), theme)
}

fn agent(colour: Rgb, theme: &Theme) -> Rgb {
    if colour == AGENT { theme.agent } else { colour }
}

/// `ink`, or the theme's text colour when `ink` would not show on its
/// background.
pub(super) fn legible(ink: Rgb, theme: &Theme) -> Rgb {
    let (a, b) = (luminance(ink), luminance(theme.background));
    let contrast = (a.max(b) + 0.05) / (a.min(b) + 0.05);
    if contrast < MIN_CONTRAST { theme.text } else { ink }
}

/// The relative luminance of an sRGB colour, 0 (black) to 1 (white).
fn luminance(c: Rgb) -> f64 {
    let linear = |v: u8| (f64::from(v) / 255.0).powf(2.2);
    0.2126 * linear(c.r) + 0.7152 * linear(c.g) + 0.0722 * linear(c.b)
}

/// The dash pattern of a line `width` wide, on and off, in the line's own
/// units; `None` for a solid line. A dot is a dash of no length, which a
/// round end draws as a dot.
pub fn pattern(dash: Dash, width: f64) -> Option<[f64; 2]> {
    match dash {
        Dash::Solid => None,
        Dash::Dashed => Some([DASH_ON * width, DASH_OFF * width]),
        Dash::Dotted => Some([0.0, DOT_EVERY * width]),
    }
}

/// An arrow's heads: for each, its tip and the ends of its two barbs, at
/// `to` for `End`, at both ends for `Both`, none for `None`.
pub(super) fn heads(from: Point, to: Point, heads: Heads) -> Vec<(Point, [Point; 2])> {
    let ends = match heads {
        Heads::End => vec![(from, to)],
        Heads::None => Vec::new(),
        Heads::Both => vec![(from, to), (to, from)],
    };
    ends.into_iter().map(|(back, tip)| (tip, barbs(back, tip))).collect()
}

/// The ends of an arrow's two barbs, in canvas points. They point back along
/// the shaft; a shaft of no length points right, so its head points left.
pub(super) fn barbs(from: Point, to: Point) -> [Point; 2] {
    let heading = (to.y - from.y).atan2(to.x - from.x);
    [heading - HEAD_ANGLE, heading + HEAD_ANGLE].map(|angle| Point {
        x: to.x - HEAD_LENGTH * angle.cos(),
        y: to.y - HEAD_LENGTH * angle.sin(),
    })
}

/// What one pass paints of a draw: its fill alone, or all but its fill.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pass {
    Fill,
    Ink,
}

/// The order `draws` (bottom first) are painted in (docs/design/figures.md,
/// "Draw order"): PDF pages, then every fill, then the rest, each in the
/// order given. A filled panel (a peek's paper, a selection) is chrome over
/// the page: it is painted over all before it, and what follows it is a
/// layer of its own, in the same order, so no fill lands under its paper.
pub fn order<'a>(draws: impl IntoIterator<Item = &'a Draw>) -> Vec<(Pass, &'a Draw)> {
    let mut out = Vec::new();
    let mut layer: Vec<&Draw> = Vec::new();
    for draw in draws {
        if matches!(draw.mark, Mark::Panel { fill: Some(_), .. }) {
            lay(&mut out, &std::mem::take(&mut layer));
            out.push((Pass::Ink, draw));
        } else {
            layer.push(draw);
        }
    }
    lay(&mut out, &layer);
    out
}

/// One layer's draws in paint order, onto `out`.
fn lay<'a>(out: &mut Vec<(Pass, &'a Draw)>, layer: &[&'a Draw]) {
    let paper = |d: &&Draw| matches!(d.mark, Mark::Pdf { .. });
    out.extend(layer.iter().copied().filter(paper).map(|d| (Pass::Ink, d)));
    out.extend(layer.iter().copied().filter(|d| fill(&d.mark).is_some()).map(|d| (Pass::Fill, d)));
    out.extend(layer.iter().copied().filter(|d| !paper(d)).map(|d| (Pass::Ink, d)));
}

/// The fill a draw's fill pass paints: a filled box's or a closed stroke's.
/// A panel paints its own fill with its outline.
pub fn fill(mark: &Mark) -> Option<Fill> {
    match *mark {
        Mark::Box { fill, .. } | Mark::Stroke { fill, .. } => fill,
        _ => None,
    }
}

#[cfg(test)]
mod tests;
