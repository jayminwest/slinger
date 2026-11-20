//! Reading a canvas without changing it: a page, a packet, the areas a
//! sling renders, crops, strokes and found texts.

use serde_json::Value;
#[cfg(doc)]
use slinger_core::canvas::file::Log;
use slinger_core::canvas::file::Shown;
use slinger_core::packet::{Area, Packet};
use slinger_core::render::{Backdrop, Image};

use super::{Canvases, Result};

impl Canvases {
    /// One page of a canvas (see [`Log::show`]) and the number of the next
    /// line the canvas will write.
    ///
    /// # Errors
    ///
    /// `not_found` for a canvas or page that is not there, and the errors of
    /// reading the file.
    pub fn show(&self, id: &str, page: Option<&str>) -> Result<(Shown, u64)> {
        self.with(id, |log| Ok((log.show(page)?, log.next())))
    }

    /// The packet of one page, or of what the box `inside` holds (see
    /// [`Log::packet`]): the lines after `after` are new, and the images of
    /// new ink are drawn over `under`.
    ///
    /// # Errors
    ///
    /// `not_found` for a canvas, page or box that is not there, `invalid`
    /// for a selected id that is not one or an `inside` that is not a box,
    /// and the errors of reading the file.
    pub fn packet(
        &self,
        id: &str,
        page: Option<&str>,
        inside: Option<&str>,
        after: u64,
        select: &[String],
        under: &[Backdrop],
    ) -> Result<Packet> {
        self.with(id, |log| Ok(log.packet(id, page, inside, after, select, under)?))
    }

    /// The areas of PDF pages under the new ink of one page, or of what the
    /// box `inside` holds (see [`Log::backdrops`]).
    ///
    /// # Errors
    ///
    /// `not_found` for a canvas, page or box that is not there, `invalid`
    /// for an `inside` that is not a box, and the errors of reading the file.
    pub fn backdrops(
        &self,
        id: &str,
        page: Option<&str>,
        inside: Option<&str>,
        after: u64,
    ) -> Result<Vec<Area>> {
        self.with(id, |log| Ok(log.backdrops(page, inside, after)?))
    }

    /// Images of what `of` names on a canvas, named after `name` (see
    /// [`Log::crop`]).
    ///
    /// # Errors
    ///
    /// The errors of [`Log::crop`] and of reading the file.
    pub fn crop(&self, id: &str, of: &str, name: &str) -> Result<Vec<Image>> {
        self.with(id, |log| Ok(log.crop(of, name)?))
    }

    /// The strokes `item` names on a canvas (see [`Log::strokes`]).
    ///
    /// # Errors
    ///
    /// The errors of [`Log::strokes`] and of reading the file.
    pub fn strokes(&self, id: &str, item: &str) -> Result<Vec<Value>> {
        self.with(id, |log| Ok(log.strokes(item)?))
    }

    /// The text items on a canvas that hold `phrase` (see [`Log::find`]).
    ///
    /// # Errors
    ///
    /// `not_found` for a canvas that is not there, and the errors of
    /// reading the file.
    pub fn find(&self, id: &str, phrase: &str) -> Result<Vec<Value>> {
        self.with(id, |log| Ok(log.find(phrase)))
    }
}
