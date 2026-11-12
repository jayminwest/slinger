//! The dotted grid (`[canvas]`, `toggle-grid`): dots fixed to the canvas, so
//! they pan and zoom with the ink, under every item of every canvas pane.
//! Display only: the grid is no item, so nothing selects, erases, slings or
//! stores it, and the images an agent reads (the core's) never show it.
//!
//! A dot stays [`RADIUS`] screen points at any zoom. Zoomed out so far that
//! dots would come closer than [`MIN_GAP`] screen points, only every 2nd,
//! 5th, 10th, 20th... dot shows, counted from the canvas origin, so the grid
//! never turns to noise and the dots that stay do not move. Which dots a
//! repaint draws is [`Grid::dots`]: only those inside the box `platform`
//! repaints, so a pen stroke's small repaint draws a few dots. Pure.

use super::canvas::Scene;
use super::style::mix;
use crate::config::canvas::{Background, Canvas};
use crate::config::{Rgb, Theme};
use crate::layout::{Point, Rect, Viewport};

/// A dot's radius, in screen points.
pub const RADIUS: f64 = 1.0;
/// The closest two shown dots come, in screen points.
const MIN_GAP: f64 = 8.0;
/// How far a dot's colour moves from the background toward the muted
/// colour, out of 255: a quiet mark, well under the ink.
const DOT: u16 = 90;
/// The most dots a step skips: past it, a pane shows no more thinning.
const MOST_STEP: u32 = 1_000_000;

/// A canvas pane's dots: `spacing` canvas points apart, in `colour`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Grid {
    pub spacing: f64,
    pub colour: Rgb,
}

impl Grid {
    /// The grid `spacing` canvas points apart in `theme`'s dot colour.
    pub fn new(spacing: f64, theme: &Theme) -> Grid {
        Grid { spacing, colour: mix(theme.background, theme.muted, DOT) }
    }

    /// Which dots show at `zoom`: every one (1), or every 2nd, 5th, 10th,
    /// 20th... the fewest skipped that keep them [`MIN_GAP`] apart.
    pub fn step(self, zoom: f64) -> u32 {
        let mut decade = 1;
        while decade < MOST_STEP {
            for k in [1, 2, 5].map(|m| m * decade) {
                if self.spacing * f64::from(k) * zoom >= MIN_GAP {
                    return k;
                }
            }
            decade *= 10;
        }
        MOST_STEP
    }

    /// The centres, in screen points, of the dots seen through `view` whose
    /// ink touches `area` (a box of the screen), row by row.
    pub fn dots(self, view: Viewport, area: Rect) -> impl Iterator<Item = Point> {
        let pitch = self.spacing * f64::from(self.step(view.zoom()));
        let from = view.to_canvas(Point { x: area.x - RADIUS, y: area.y - RADIUS });
        let to = view
            .to_canvas(Point { x: area.x + area.width + RADIUS, y: area.y + area.height + RADIUS });
        // Dot `n` of a row or column is at `n * pitch`. The casts saturate,
        // and a pitch that is not a positive number shows nothing.
        #[expect(clippy::cast_possible_truncation, reason = "a row or column's index")]
        let index = |v: f64, round: fn(f64) -> f64| round(v / pitch) as i64;
        let ok = pitch.is_finite() && pitch > 0.0;
        let span = |lo: f64, hi: f64| {
            let (lo, hi) = if ok { (index(lo, f64::ceil), index(hi, f64::floor)) } else { (1, 0) };
            lo..=hi
        };
        let (columns, rows) = (span(from.x, to.x), span(from.y, to.y));
        #[expect(clippy::cast_precision_loss, reason = "an index far below 2^52")]
        let at = move |n: i64| n as f64 * pitch;
        rows.flat_map(move |j| {
            columns.clone().map(move |i| view.to_screen(Point { x: at(i), y: at(j) }))
        })
    }
}

/// Whether the canvas panes show the grid: the config's `[canvas]` says at
/// start, and `toggle-grid` flips it for the running app. The toggle is not
/// written anywhere, as the sidebar's is not: a config edit that changes
/// `background` sets it again, and one that changes only `grid` keeps it.
#[derive(Debug, Clone, Default)]
pub struct Paper {
    config: Option<Canvas>,
    dots: bool,
}

impl Paper {
    /// Takes the config's `[canvas]`. The first, or a new `background`,
    /// sets whether the dots show.
    pub fn follow(&mut self, config: Canvas) {
        if self.config.map(|c| c.background) != Some(config.background) {
            self.dots = config.background == Background::Dots;
        }
        self.config = Some(config);
    }

    /// Shows the dots, or hides them; what to flash.
    pub fn toggle(&mut self) -> &'static str {
        self.dots = !self.dots;
        if self.dots { "Dotted grid on" } else { "Dotted grid off" }
    }

    /// The grid the panes show in `theme`, if they show one.
    pub fn grid(&self, theme: &Theme) -> Option<Grid> {
        let spacing = self.config.unwrap_or_default().grid.points();
        self.dots.then(|| Grid::new(spacing, theme))
    }
}

impl Scene {
    /// The scene on `grid`'s paper. When the paper is not what `last` had,
    /// the whole pane changed.
    #[must_use]
    pub fn on(mut self, grid: Option<Grid>, last: Option<&Scene>) -> Scene {
        if last.is_some_and(|last| last.grid != grid) {
            self.changed = Some(self.screen.rect());
        }
        self.grid = grid;
        self
    }
}

#[cfg(test)]
mod tests;
