//! A stored canvas's packet, for `canvas read`: the request names things the
//! way the file does (line numbers, page ids or names), and the packet's op
//! range is in the file's line numbers.

use super::Log;
use crate::canvas::{ItemId, Marker};
use crate::packet::{Ask, Packet, build};
use crate::{Error, Result};

/// The letters an item id may start with: the file's `i`, and the packet's
/// `c` (a cluster: its earliest stroke), `t`, `b`, `a`, `l` and `u`.
const ID_LETTERS: [char; 7] = ['i', 'c', 't', 'b', 'a', 'l', 'u'];

impl Log {
    /// The packet of one page of this canvas, `id`: the page `page` names by
    /// id (`p<n>`) or name, or the first. The lines numbered above `after`
    /// are new; `after` 0 makes everything new, and one at or past the last
    /// line makes nothing new (B4). `select`, when not empty, narrows the
    /// `page` section (see [`Ask::selection`]); its ids are the file's
    /// (`i12`) or the packet's (`c12`, `t12`, `b12`, `a12`, `l12`, `u12`).
    ///
    /// # Errors
    ///
    /// [`Error::NotFound`] for a page that is not there, and
    /// [`Error::Invalid`] for a selected id that is not an item id.
    pub fn packet(
        &self,
        id: &str,
        page: Option<&str>,
        after: u64,
        select: &[String],
    ) -> Result<Packet> {
        let page = self.page(page)?.id();
        let selection = select.iter().map(|s| item(s)).collect::<Result<Vec<_>>>()?;
        let (ops, lines) = self.applied();
        // Ops and lines differ only where a line changes several items or none.
        let since = Marker(lines.partition_point(|n| *n <= after) as u64);
        let mut packet = build(ops, &Ask { canvas: id, page, since, selection: &selection })?;
        let last = self.next().saturating_sub(1);
        packet.header.from = after.min(last) + 1;
        packet.header.to = last;
        packet.count();
        Ok(packet)
    }
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
