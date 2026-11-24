//! What a drawing script sees of the canvas where its figure goes
//! (docs/design/figures.md, "The script API (v1)", rule 5): the anchor,
//! the item it goes beside or over (its size and text), and, with `--over`
//! or `--at`, the bounds of the items around it in the figure's own
//! coordinates. The script reads them as the globals `anchor` and
//! `around`. Only those two placements know the figure's origin before the
//! script runs, so only they give neighbours.

use super::{Again, Log, Placement, json as file_json, look};
use crate::Result;
use crate::canvas::{ItemId, PageId, Rect, Shape};

/// How far from the item drawn over, or from the point drawn at, an item
/// may be and still be around the figure, in canvas units.
pub const REACH: f64 = 500.0;

/// The most items a script is told are around it, nearest first.
pub const AROUND: usize = 500;

/// What a script sees round its figure.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Surround {
    /// The item the figure goes beside or over.
    pub anchor: Option<Anchor>,
    /// The items within [`REACH`] of where it goes, nearest first, at
    /// most [`AROUND`]: none beside an item or in a box.
    pub around: Vec<Near>,
}

/// The item a figure goes beside or over, as a script sees it.
#[derive(Debug, Clone, PartialEq)]
pub struct Anchor {
    /// The id or the figure address it was named by.
    pub id: String,
    /// Its kind as an item object names it (`text`, `box`, ...), or
    /// `figure` / `part` for an address.
    pub kind: &'static str,
    /// Its size in canvas units.
    pub w: f64,
    pub h: f64,
    /// Its words, for a text.
    pub text: Option<String>,
}

/// An item near a figure: its id, kind and bounds, in canvas units from
/// the figure's origin.
#[derive(Debug, Clone, PartialEq)]
pub struct Near {
    pub id: String,
    pub kind: &'static str,
    pub frame: Rect,
}

impl Log {
    /// What the script of the figure `name`, placed as `at` says, sees.
    ///
    /// # Errors
    ///
    /// Those of an id or a page that names nothing or is not one.
    pub fn surround(&self, name: &str, at: &Placement) -> Result<Surround> {
        Ok(match at {
            Placement::Beside(id) => {
                Surround { anchor: Some(self.anchor(id)?), around: Vec::new() }
            }
            Placement::Over(id) => {
                let (_, frame, page) = self.named(id)?;
                let skip = self.resolve(id)?;
                let around = self.near(name, page, frame, &skip);
                Surround { anchor: Some(self.anchor(id)?), around }
            }
            Placement::In(_) => Surround::default(),
            Placement::At { page, x, y } => {
                let page = self.page(page.as_deref())?.id();
                let point = Rect::new(file_json::units(*x), file_json::units(*y), 0.0, 0.0);
                Surround { anchor: None, around: self.near(name, page, point, &[]) }
            }
        })
    }

    /// What the script of the figure `name` sees when it is drawn
    /// `again`: its first draw went beside or over `anchor` (if any) and
    /// over a frame of `target`'s size (tenths, if any). An anchor that is
    /// gone is none; the neighbours are found where the figure is now.
    #[must_use]
    pub fn surround_again(
        &self,
        name: &str,
        again: &Again,
        anchor: Option<&str>,
        target: Option<[i64; 4]>,
    ) -> Surround {
        let found = anchor.and_then(|id| self.anchor(id).ok());
        let Ok(page) = file_json::parse_page(&again.page) else { return Surround::default() };
        let [x, y] = again.origin.map(file_json::units);
        let region = match (anchor, target) {
            (Some(_), Some([_, _, w, h])) => {
                Rect::new(x, y, file_json::units(w), file_json::units(h))
            }
            (None, None) => Rect::new(x, y, 0.0, 0.0),
            _ => return Surround { anchor: found, around: Vec::new() },
        };
        let skip = anchor.and_then(|id| self.resolve(id).ok()).unwrap_or_default();
        Surround { anchor: found, around: self.near(name, page, region, &skip) }
    }

    fn anchor(&self, id: &str) -> Result<Anchor> {
        let (item, frame, _) = self.named(id)?;
        let shape = self.canvas().item(item).map(crate::canvas::Item::shape);
        let (kind, text) = match shape {
            _ if look::number(id).is_none() => {
                (if id.contains('/') { "part" } else { "figure" }, None)
            }
            Some(Shape::Text { text, .. }) => ("text", Some(text.clone())),
            Some(shape) => (kind(shape), None),
            None => ("figure", None),
        };
        Ok(Anchor { id: id.to_owned(), kind, w: frame.w, h: frame.h, text })
    }

    /// The items shown on `page` within [`REACH`] of `region`, but those
    /// in `skip` and in the figure `name`, nearest first, from the
    /// region's top left.
    fn near(&self, name: &str, page: PageId, region: Rect, skip: &[ItemId]) -> Vec<Near> {
        let mut near: Vec<(f64, Near)> = self
            .canvas()
            .visible(page)
            .filter(|i| !skip.contains(&i.id()))
            .filter(|i| i.member().fig.as_deref() != Some(name))
            .filter_map(|i| {
                let b = i.bounds();
                let gap = distance(b, region);
                let frame = Rect::new(b.x - region.x, b.y - region.y, b.w, b.h);
                let id = file_json::item_id(i.id());
                (gap <= REACH).then(|| (gap, Near { id, kind: kind(i.shape()), frame }))
            })
            .collect();
        near.sort_by(|a, b| a.0.total_cmp(&b.0));
        near.into_iter().take(AROUND).map(|(_, n)| n).collect()
    }
}

/// The kind's name as an item object writes it.
fn kind(shape: &Shape) -> &'static str {
    match shape {
        Shape::Stroke { .. } => "stroke",
        Shape::Text { .. } => "text",
        Shape::Box { .. } => "box",
        Shape::Arrow { .. } => "arrow",
        Shape::Live { .. } => "live",
        Shape::Button { .. } => "button",
        Shape::Pdf { .. } => "pdf",
    }
}

/// The gap between two rectangles; zero when they touch or overlap.
fn distance(a: Rect, b: Rect) -> f64 {
    let dx = (b.x - (a.x + a.w)).max(a.x - (b.x + b.w)).max(0.0);
    let dy = (b.y - (a.y + a.h)).max(a.y - (b.y + b.h)).max(0.0);
    dx.hypot(dy)
}
