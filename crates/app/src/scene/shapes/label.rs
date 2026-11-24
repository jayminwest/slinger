//! Chrome's lines of text over a page: the hint under the text being
//! typed, the selection's count. Pure.

use crate::config::Rgb;
use crate::layout::{Point, Rect};
use crate::scene::canvas::{Item, ItemId, Shape};

/// How high chrome's labels are set (the selection's count, the hint), and
/// how tall their line is, in screen points.
const LABEL_SIZE: f64 = 11.0;
const LABEL_LINE: f64 = 1.3;

/// A line of chrome's text, `id`, from `at` (its top left), set
/// [`LABEL_SIZE`] screen points high at `zoom`, in `colour`.
pub fn label(id: ItemId, text: String, colour: Rgb, at: Point, zoom: f64) -> Item {
    let size = LABEL_SIZE / zoom;
    let count = u32::try_from(text.chars().count()).map_or(f64::from(u32::MAX), f64::from);
    // A cell of the full size a character: the line never wraps.
    let frame = Rect { x: at.x, y: at.y, width: count.max(1.0) * size, height: label_height(zoom) };
    Item::plain(id, colour, Shape::Text { frame, text, size, column: None })
}

/// How tall a [`label`] is at `zoom`, in canvas units.
pub fn label_height(zoom: f64) -> f64 {
    LABEL_SIZE * LABEL_LINE / zoom
}
