//! Figures on a canvas file: what a script sees of the canvas before it
//! runs, and its shapes placed as one figure (see [`Log::place_figure`]).
//! The script runs outside the canvas's lock, between the two.

use serde_json::Value;
use slinger_core::canvas::Figure;
use slinger_core::canvas::file::{Again, Draft, Drawn, Log, Placement, Written, parse_author};

use super::{Canvases, Result, Wrote, now};

impl Wrote for Drawn {
    fn written(&self) -> &Written {
        &self.written
    }
}

impl Canvases {
    /// The name of the page a figure placed as `at` says goes on, and the
    /// frame it draws over or in (see [`Log::target`]), in tenths.
    ///
    /// # Errors
    ///
    /// Those of [`Log::figure_page`] and [`Log::target`] and of the file.
    pub fn figure_view(&self, id: &str, at: &Placement) -> Result<(String, Option<[i64; 4]>)> {
        self.with(id, |log| Ok((log.figure_page(at)?, log.target(at)?)))
    }

    /// The figure `name` that `items` make, as `by` would draw it, without
    /// writing it (see [`Log::draft_figure`]).
    ///
    /// # Errors
    ///
    /// Those of [`Log::draft_figure`] and of the file.
    pub fn draft_figure(
        &self,
        id: &str,
        by: &str,
        name: &str,
        items: &[Value],
        at: &Placement,
    ) -> Result<Draft> {
        let by = parse_author(by)?;
        self.with(id, |log| Ok(log.draft_figure(&by, now(), name, items, at)?))
    }

    /// Draws the figure `name` as `by`, as one undo step under the key
    /// `once` (see [`Log::place_figure`]); a draw that changes nothing
    /// writes nothing.
    ///
    /// # Errors
    ///
    /// Those of [`Log::place_figure`] and of the file.
    pub fn draw(
        &self,
        id: &str,
        by: &str,
        (name, items): (&str, &[Value]),
        at: &Placement,
        once: Option<&str>,
    ) -> Result<Drawn> {
        let by = parse_author(by)?;
        self.change(id, |log: &mut Log| Ok(log.place_figure(&by, now(), name, items, at, once)?))
    }

    /// Where `by` draws the figure `name` again, whose last draw put its
    /// (0,0) at `origin` and its frame at `frame` (see [`Log::again`]).
    ///
    /// # Errors
    ///
    /// Those of [`Log::again`] and of the file.
    pub fn again(
        &self,
        id: &str,
        by: &str,
        name: &str,
        (origin, frame): ([i64; 2], [i64; 4]),
    ) -> Result<Again> {
        let by = parse_author(by)?;
        self.with(id, |log| Ok(log.again(&by, name, origin, frame)?))
    }

    /// The figures on the canvas, as [`slinger_core::canvas::Canvas::figures`] lists them.
    ///
    /// # Errors
    ///
    /// Those of the file.
    pub fn figures(&self, id: &str) -> Result<Vec<Figure>> {
        self.with(id, |log| Ok(log.canvas().figures()))
    }
}
