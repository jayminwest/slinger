//! Figures on a canvas file: what a script sees of the canvas before it
//! runs, and its shapes placed as one figure (see [`Log::place_figure`]).
//! The script runs outside the canvas's lock, between the two.

use serde_json::Value;
use slinger_core::canvas::Figure;
use slinger_core::canvas::file::{
    Again, Draft, Drawn, Log, Placement, Surround, Written, parse_author,
};

use super::{Canvases, Result, Wrote, now};

/// What a script sees before it runs: its page's name, the frame it draws
/// over or in (tenths), its anchor and the items around it.
pub struct Sight {
    pub page: String,
    pub target: Option<[i64; 4]>,
    pub surround: Surround,
}

impl Wrote for Drawn {
    fn written(&self) -> &Written {
        &self.written
    }
}

impl Canvases {
    /// What the script of the figure `name` placed as `at` sees: the
    /// name of the page it goes on, the frame it draws over or in (see
    /// [`Log::target`]) in tenths, and its anchor and the items around it
    /// (see [`Log::surround`]).
    ///
    /// # Errors
    ///
    /// Those of [`Log::figure_page`], [`Log::target`] and
    /// [`Log::surround`] and of the file.
    pub fn figure_view(&self, id: &str, name: &str, at: &Placement) -> Result<Sight> {
        self.with(id, |log| {
            let (page, target) = (log.figure_page(at)?, log.target(at)?);
            Ok(Sight { page, target, surround: log.surround(name, at)? })
        })
    }

    /// What the script of the figure `name` sees when it is drawn `again`,
    /// its first draw beside or over `anchor` and over or in `target`
    /// (see [`Log::surround_again`]).
    ///
    /// # Errors
    ///
    /// Those of [`Log::figure_page`] and of the file.
    pub fn figure_view_again(
        &self,
        id: &str,
        name: &str,
        again: &Again,
        (anchor, target): (Option<&str>, Option<[i64; 4]>),
    ) -> Result<Sight> {
        self.with(id, |log| {
            let surround = log.surround_again(name, again, anchor, target);
            Ok(Sight { page: log.figure_page(&again.at)?, target, surround })
        })
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
