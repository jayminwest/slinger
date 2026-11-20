//! A stored canvas's packet, for `canvas read`: the request names things the
//! way the file does (line numbers, page ids or names), and the packet's op
//! range is in the file's line numbers. A sling over a PDF asks first which
//! areas to render ([`Log::backdrops`]), then passes them back ([`backdrop`]).

use super::Log;
use super::json::{tenths, units};
use crate::canvas::{ItemId, Marker, PageId, Rect};
use crate::packet::{Area, Ask, Packet, backdrops, build};
use crate::render::{Backdrop, Word};
use crate::{Error, Result};

/// The letters an item id may start with: the file's `i`, and the packet's
/// `c` (a cluster: its earliest stroke), `t`, `b`, `a`, `l`, `u` and `d`.
pub(super) const ID_LETTERS: [char; 8] = ['i', 'c', 't', 'b', 'a', 'l', 'u', 'd'];

impl Log {
    /// The packet of one page of this canvas, `id`: the page `page` names by
    /// id (`p<n>`) or name, or the first. With `inside` (a box's id, `b4` or
    /// `i4`), the packet of what that box holds, open or folded, on its page
    /// ([`Ask::inside`]); `page`, when given too, must be the box's. The lines numbered above `after`
    /// are new; `after` 0 makes everything new, and one at or past the last
    /// line makes nothing new (B4). `select`, when not empty, narrows the
    /// `page` section (see [`Ask::selection`]); its ids are the file's
    /// (`i12`) or the packet's (`c12`, `t12`, `b12`, `a12`, `l12`, `u12`, `d12`).
    /// `under` is the page under the new ink ([`Ask::backdrops`]).
    ///
    /// # Errors
    ///
    /// [`Error::NotFound`] for a page or a box that is not there, and
    /// [`Error::Invalid`] for a selected id that is not an item id, or an
    /// `inside` that is not a box.
    pub fn packet(
        &self,
        id: &str,
        page: Option<&str>,
        inside: Option<&str>,
        after: u64,
        select: &[String],
        under: &[Backdrop],
    ) -> Result<Packet> {
        let (page, inside) = self.scope(page, inside)?;
        let selection = select.iter().map(|s| item(s)).collect::<Result<Vec<_>>>()?;
        let ask = Ask {
            canvas: id,
            page,
            since: self.since(after),
            selection: &selection,
            backdrops: under,
            inside,
        };
        let mut packet = build(self.applied().0, &ask)?;
        let last = self.next().saturating_sub(1);
        packet.header.from = after.min(last) + 1;
        packet.header.to = last;
        packet.count();
        Ok(packet)
    }

    /// The areas of `pdf` items under the new ink of a page's packet, or
    /// of a box's (see [`Log::packet`] for `page`, `inside` and `after`, and
    /// [`backdrops`]): with `inside`, only the ink the box holds.
    ///
    /// # Errors
    ///
    /// As [`Log::packet`]: [`Error::NotFound`] for a page or a box that is
    /// not there, [`Error::Invalid`] for an `inside` that is not a box.
    pub fn backdrops(
        &self,
        page: Option<&str>,
        inside: Option<&str>,
        after: u64,
    ) -> Result<Vec<Area>> {
        let (page, inside) = self.scope(page, inside)?;
        let ask = Ask {
            canvas: "",
            page,
            since: self.since(after),
            selection: &[],
            backdrops: &[],
            inside,
        };
        backdrops(self.applied().0, &ask)
    }

    /// The page a packet reads and the box it reads inside: the box's own
    /// page when `inside` names a box and `page` is not given.
    fn scope(&self, page: Option<&str>, inside: Option<&str>) -> Result<(PageId, Option<ItemId>)> {
        let inside = inside.map(item).transpose()?;
        let holder = inside.and_then(|b| self.canvas().item(b));
        let page = match (holder, page) {
            (Some(b), None) => b.page(),
            _ => self.page(page)?.id(),
        };
        Ok((page, inside))
    }

    /// The op marker after the file's line `after`. Ops and lines differ
    /// only where a line changes several items or none.
    fn since(&self, after: u64) -> Marker {
        Marker(self.applied().1.partition_point(|n| *n <= after) as u64)
    }
}

/// A backdrop as a request gives it: the item as `canvas show` or a packet
/// names it, the area `[x, y, w, h]` in tenths, the raw RGB and the words
/// file's bytes.
///
/// # Errors
///
/// [`Error::Invalid`] for an item id that is not one, an area with no size,
/// RGB of the wrong length or words that do not parse.
pub fn backdrop(
    item_id: &str,
    area: [i64; 4],
    rgb: Vec<u8>,
    words: Option<&[u8]>,
) -> Result<Backdrop> {
    let [x, y, w, h] = area.map(units);
    if w <= 0.0 || h <= 0.0 {
        return Err(Error::Invalid {
            message: format!("the backdrop area {area:?} has no size"),
            hint: Some("pass the area as sling_backdrops gave it".into()),
        });
    }
    let words = words.map(Word::read).transpose()?.unwrap_or_default();
    Backdrop::new(item(item_id)?, Rect::new(x, y, w, h), rgb, words)
}

/// A rectangle as `[x, y, w, h]` in tenths, as the wire carries it.
#[must_use]
pub fn rect_tenths(r: Rect) -> [i64; 4] {
    [r.x, r.y, r.w, r.h].map(tenths)
}

/// A size `(w, h)` as `[w, h]` in tenths, as the wire carries it.
#[must_use]
pub fn size_tenths((w, h): (f64, f64)) -> [i64; 2] {
    [w, h].map(tenths)
}

/// An item id as the file or a packet writes it.
fn item(id: &str) -> Result<ItemId> {
    let digits = id.strip_prefix(ID_LETTERS).filter(|d| !d.is_empty());
    let number =
        digits.filter(|d| d.bytes().all(|b| b.is_ascii_digit())).and_then(|d| d.parse().ok());
    number.map(ItemId::new).ok_or_else(|| Error::Invalid {
        message: format!("{id:?} is not an item id"),
        hint: Some(
            "pass ids as a packet or 'slinger canvas show' writes them: c42, t12, i7".into(),
        ),
    })
}
