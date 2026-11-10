//! Builds a packet: a canvas's log, a page, a marker and a selection in; the
//! packet out. Hides the delta, clustering, rendering and the index.
//!
//! Clustering runs over the whole page, not the new ink alone. A size unit is
//! the median stroke of a pen, and a few new strokes are too few to measure
//! one: a lone new stroke would be its own unit and could never be long. The
//! whole page also gives ink the same cluster, and so the same id, in `new`
//! and `page`, and a stroke added to a note within [`cluster`]'s pause joins
//! that note, so the note's image is whole.

use std::collections::{BTreeMap, BTreeSet};

use super::format::{Header, Packet};
use super::{Index, cluster};
use crate::canvas::{Canvas, Item, ItemId, Marker, Op, PageId, changes_since, replay};
use crate::{Error, Result, render};

/// What to read.
#[derive(Debug, Clone, Copy)]
pub struct Ask<'a> {
    /// The canvas id, for the header.
    pub canvas: &'a str,
    pub page: PageId,
    /// The marker of the last sling: ops after it are new. `Marker(0)` makes
    /// everything new.
    pub since: Marker,
    /// When not empty, the `page` section keeps only the selected items and
    /// the clusters holding a selected stroke. `new` and `made` are whole.
    pub selection: &'a [ItemId],
}

/// The packet of `ask.page` on the canvas that `log` builds.
///
/// Nothing new is a valid packet: `new` and `made` are empty and `from` is
/// past `to` (B4).
///
/// # Errors
///
/// [`Error::NotFound`] when the page is not on the canvas.
pub fn build(log: &[Op], ask: &Ask<'_>) -> Result<Packet> {
    let canvas = replay(Canvas::new(), log);
    let page = canvas.page(ask.page).ok_or_else(|| Error::NotFound {
        what: format!("page {}", ask.page),
        hint: Some("list the canvas's pages for their ids".into()),
    })?;
    let delta = changes_since(log, ask.since);
    let changed: BTreeSet<ItemId> = delta.new.iter().chain(&delta.changed).copied().collect();
    let clusters = cluster(&canvas, canvas.items(ask.page).map(Item::id));
    let mut index = Index::new(&canvas, ask.page, &clusters, |id| changed.contains(&id));
    if !ask.selection.is_empty() {
        let first: BTreeMap<ItemId, u64> =
            clusters.iter().flat_map(|c| c.iter().map(|s| (*s, c[0].get()))).collect();
        let keep: BTreeSet<u64> =
            ask.selection.iter().map(|id| first.get(id).copied().unwrap_or(id.get())).collect();
        index.narrow(|n| keep.contains(&n));
    }
    let mut images = Vec::new();
    let mut names: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for r in index.render() {
        let drawn = render::items(&canvas, &r.strokes, &r.id);
        names.insert(r.id.clone(), drawn.iter().map(|i| i.name.clone()).collect());
        images.extend(drawn);
    }
    let header = Header {
        canvas: ask.canvas.to_string(),
        page: page.name().to_string(),
        from: delta.from.0 + 1,
        to: delta.to.0,
        scale: render::SCALE,
        at: None,
    };
    let body = index.text(|id| names.get(id).cloned().unwrap_or_default());
    let mut packet = Packet::read(&format!("{}{body}", Packet::header_line(&header)))?;
    packet.images = images;
    Ok(packet)
}
