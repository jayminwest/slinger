//! What an agent asks a canvas for when the packet is not enough (SPEC R5,
//! R6): an image of one item or one box, and the raw points of ink.
//!
//! Ids are the file's (`i12`) or the packet's (`c12`, `t12`, `b12`, `a12`):
//! the number is the item's, so every letter names that item, except that
//! `c<n>` on a stroke of the user's names the whole cluster it is in on the
//! page as it is now (`packet::cluster`). A box is `x,y,w,h` in the file's
//! units, integers in tenths of a unit, on the first page, or `<page>:x,y,w,h`
//! on the page with that id or name. Images follow `render`'s rules, so a crop
//! is at the packet's scale and large ones come as tiles. A crop is at most
//! [`MAX_CROP`] units a side, so no request can ask for a flood of tiles.

use serde_json::Value;

use super::Log;
use super::json;
use crate::canvas::{Author, Item, ItemId, Rect, Shape};
use crate::packet::cluster;
use crate::render::{self, Image, MARGIN};
use crate::{Error, Result};

/// The longest side of a crop, in canvas units: at most 4 by 4 tiles.
pub const MAX_CROP: f64 = 16_000.0;

impl Log {
    /// Images of what `of` names, an item or a box (see the module doc),
    /// named after `name`: `<name>.png`, or `<name>-<n>.png` for tiles. An
    /// item is framed with `render`'s margin; a box is cut at its edges, and
    /// one with nothing in it gives a white image.
    ///
    /// # Errors
    ///
    /// `invalid` for an `of` that is neither, a negative size, or a crop over
    /// [`MAX_CROP`], and `not_found` for an item or page that is not there.
    pub fn crop(&self, of: &str, name: &str) -> Result<Vec<Image>> {
        if !of.contains(',') {
            let items = self.resolve(of)?;
            let frame = self.canvas().bounds_of(items.iter().copied()).unwrap_or_default();
            fits(frame.w + 2.0 * MARGIN, frame.h + 2.0 * MARGIN, "crop part of it with a box")?;
            return Ok(render::items(self.canvas(), &items, name));
        }
        let (page, area) = of.rsplit_once(':').map_or((None, of), |(p, a)| (Some(p), a));
        let numbers: Vec<i64> = area.split(',').map_while(|n| n.trim().parse().ok()).collect();
        let [x, y, w, h] = <[i64; 4]>::try_from(numbers)
            .map_err(|_| invalid(format!("{of:?} is not a box"), BOX_HINT))?
            .map(json::units);
        if w < 0.0 || h < 0.0 {
            return Err(invalid(format!("the box {of:?} has a negative size"), BOX_HINT));
        }
        fits(w, h, "crop a smaller box")?;
        let page = self.page(page)?.id();
        Ok(render::crop(self.canvas(), page, Rect::new(x, y, w, h), name))
    }

    /// The strokes `id` names, in drawing order, each as `canvas show` gives
    /// an item but with `points`: absolute `[x, y]` pairs in tenths, in
    /// place of the file's `pts`.
    ///
    /// # Errors
    ///
    /// `invalid` for an id that is not one or an item that is not ink, and
    /// `not_found` for an item that is not on the canvas.
    pub fn strokes(&self, id: &str) -> Result<Vec<Value>> {
        let canvas = self.canvas();
        let items = self.resolve(id)?;
        let found = items.iter().filter_map(|i| canvas.item(*i));
        found
            .map(|item| {
                let Shape::Stroke { points, .. } = item.shape() else {
                    return Err(invalid(format!("{id} is not ink"), STROKES_HINT));
                };
                let mut whole: Value =
                    serde_json::from_str(&json::whole_item(item)).unwrap_or_default();
                let pairs = points.iter().map(|p| [json::tenths(p.x), json::tenths(p.y)]);
                if let Some(fields) = whole.as_object_mut() {
                    fields.remove("pts");
                    fields.insert("points".into(), pairs.collect::<Vec<_>>().into());
                }
                Ok(whole)
            })
            .collect()
    }

    /// The items `id` names, in drawing order. See the module doc.
    fn resolve(&self, id: &str) -> Result<Vec<ItemId>> {
        let canvas = self.canvas();
        let number = id
            .strip_prefix(['i', 'c', 't', 'b', 'a'])
            .filter(|d| d.bytes().all(|b| b.is_ascii_digit()))
            .and_then(|d| d.parse().ok());
        let Some(n) = number else {
            return Err(invalid(format!("{id:?} is not an item id"), ID_HINT));
        };
        let Some(item) = canvas.item(ItemId::new(n)) else {
            return Err(Error::NotFound { what: format!("item {id}"), hint: Some(ID_HINT.into()) });
        };
        if !id.starts_with('c') || !is_ink(item) || *item.author() != Author::User {
            return Ok(vec![item.id()]);
        }
        let mut clusters = cluster(canvas, canvas.items(item.page()).map(Item::id));
        let found = clusters.iter().position(|c| c.contains(&item.id()));
        Ok(found.map_or_else(|| vec![item.id()], |c| clusters.swap_remove(c)))
    }
}

const ID_HINT: &str = "pass an id from 'slinger canvas show' (i12) or from a packet (c12, t12)";

const BOX_HINT: &str = "give a box as x,y,w,h in tenths of a unit, after <page>: for another page";

const STROKES_HINT: &str = "only ink has points: pass a stroke (i12) or a cluster (c12)";

fn is_ink(item: &Item) -> bool {
    matches!(item.shape(), Shape::Stroke { .. })
}

/// Refuses a crop over [`MAX_CROP`] a side.
fn fits(w: f64, h: f64, hint: &str) -> Result<()> {
    if w <= MAX_CROP && h <= MAX_CROP {
        return Ok(());
    }
    let what = format!("a crop of {w} by {h} units, over the {MAX_CROP} a side a crop may be");
    Err(invalid(what, hint))
}

fn invalid(message: String, hint: &str) -> Error {
    Error::Invalid { message, hint: Some(hint.into()) }
}
