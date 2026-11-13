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
use super::{Index, cluster, words};
use crate::canvas::{Canvas, Changes, Item, ItemId, Marker, Op, PageId, changes_since, replay};
use crate::render::Backdrop;
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
    /// [`Packet::selected`] names the selection by packet id.
    pub selection: &'a [ItemId],
    /// The page under the new ink, as the app rendered it
    /// ([`super::backdrops`] says which areas). Each image of a new cluster
    /// is drawn over the ones that meet it, and the words near a cluster's
    /// ink end its line. Empty: ink on white, no words.
    pub backdrops: &'a [Backdrop],
}

/// What a packet and its backdrops are read from: the canvas the log
/// builds, what changed after the marker, its clusters, and the index.
pub(super) struct Read {
    pub canvas: Canvas,
    pub page_name: String,
    pub delta: Changes,
    pub clusters: Vec<Vec<ItemId>>,
    pub index: Index,
}

impl Read {
    pub fn new(log: &[Op], ask: &Ask<'_>) -> Result<Self> {
        let canvas = replay(Canvas::new(), log);
        let page = canvas.page(ask.page).ok_or_else(|| Error::NotFound {
            what: format!("page {}", ask.page),
            hint: Some("list the canvas's pages for their ids".into()),
        })?;
        let page_name = page.name().to_string();
        let delta = changes_since(log, ask.since);
        let changed: BTreeSet<ItemId> = delta.new.iter().chain(&delta.changed).copied().collect();
        let clusters = cluster(&canvas, canvas.items(ask.page).map(Item::id));
        let index = Index::new(&canvas, ask.page, &clusters, |id| changed.contains(&id));
        Ok(Self { canvas, page_name, delta, clusters, index })
    }
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
    let Read { canvas, page_name, delta, clusters, mut index } = Read::new(log, ask)?;
    // A selected stroke stands for its cluster, whose number is its first stroke's.
    let first: BTreeMap<ItemId, u64> =
        clusters.iter().flat_map(|c| c.iter().map(|s| (*s, c[0].get()))).collect();
    let mut selected: Vec<u64> = Vec::new();
    for id in ask.selection {
        let n = first.get(id).copied().unwrap_or(id.get());
        if !selected.contains(&n) {
            selected.push(n);
        }
    }
    if !selected.is_empty() {
        let keep: BTreeSet<u64> = selected.iter().copied().collect();
        index.narrow(|n| keep.contains(&n));
    }
    let mut images = Vec::new();
    let mut names: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let under = render::usable(&canvas, ask.page, ask.backdrops);
    let mut said = Vec::new();
    for r in index.render() {
        let drawn = render::over(&canvas, &r.strokes, &r.id, ask.backdrops);
        names.insert(r.id.clone(), drawn.iter().map(|i| i.name.clone()).collect());
        images.extend(drawn);
        let ink = r.strokes.iter().filter_map(|s| canvas.item(*s)).map(Item::bounds);
        if let Some(text) = ink.reduce(render::union).and_then(|b| words::near(b, &under)) {
            said.push((r.id.clone(), text));
        }
    }
    for (id, text) in said {
        index.say(&id, text);
    }
    let header = Header {
        canvas: ask.canvas.to_string(),
        page: page_name,
        from: delta.from.0 + 1,
        to: delta.to.0,
        scale: render::SCALE,
        tokens: None,
        at: None,
    };
    let body = index.text(|id| names.get(id).cloned().unwrap_or_default());
    let mut packet = Packet::read(&format!("{}{body}", Packet::header_line(&header)))?;
    packet.images = images;
    // Every number in a packet is on one line, so a number names its line's id.
    let lines = || packet.new.iter().chain(&packet.made).chain(&packet.page);
    let number = |id: &str| id.trim_start_matches(|c: char| c.is_ascii_alphabetic()).parse().ok();
    packet.selected = selected
        .iter()
        .filter_map(|n| lines().find(|e| number(&e.id) == Some(*n)).map(|e| e.id.clone()))
        .collect();
    packet.count();
    Ok(packet)
}
