//! The look of the user's marks a put goes beside: the colour and text size
//! an agent's reply takes when it gives none, so it matches the ink near it.
//!
//! The rule: a put takes the colour and text size of the user's ink or text
//! it goes beside, else of the user's ink and text on that page, else black
//! and [`TEXT_SIZE`](crate::canvas::TEXT_SIZE). The user's marks are the
//! user's strokes and text items; a stroke counts with its whole cluster
//! (`packet::cluster`), so `i12` and `c12` on the same note look the same.
//!
//! The colour is the one most of those marks have (the earliest on a tie).
//! The size is the median of their sizes: a text's own size, and for each
//! cluster of ink the median height of its lines of writing. The page's
//! colour is not known to the core, so matching the user's ink is how a reply
//! shows on any paper: the user's ink already does.

use std::collections::BTreeSet;

use crate::canvas::{Author, Canvas, Colour, Item, ItemId, PageId, Shape, TEXT_SIZES};
use crate::packet::cluster;

/// Over how many times a cluster's median stroke height a stroke is too
/// tall to be writing (a frame, a bracket, a long arrow) and is left out of
/// its lines. Letters with ascenders and descenders are about twice the
/// height of those without, so writing stays in.
const TALL: f64 = 2.0;

/// The colour and text size, in canvas units, the user's marks near
/// `beside` have on `page`; `None` for each when there are no marks.
pub(super) fn look(
    canvas: &Canvas,
    page: PageId,
    beside: &[ItemId],
) -> (Option<Colour>, Option<f64>) {
    let mine = |i: &&Item| *i.author() == Author::User && is_mark(i);
    let named: Vec<&Item> = beside.iter().filter_map(|&id| canvas.item(id)).filter(mine).collect();
    let marks: Vec<&Item> =
        if named.is_empty() { canvas.items(page).filter(mine).collect() } else { named };
    let strokes = marks.iter().filter(|i| matches!(i.shape(), Shape::Stroke { .. }));
    let clusters = if strokes.clone().next().is_none() {
        Vec::new()
    } else {
        // A stroke's cluster as the packet sees it: clustered with the
        // whole page, kept when it holds a mark.
        let wanted: BTreeSet<ItemId> = strokes.map(|i| i.id()).collect();
        let all = canvas.items(page).filter(mine).map(Item::id);
        cluster(canvas, all).into_iter().filter(|c| c.iter().any(|s| wanted.contains(s))).collect()
    };
    let mut seen: Vec<&Item> = marks.iter().copied().filter(|i| !is_stroke(i)).collect();
    seen.extend(clusters.iter().flatten().filter_map(|&id| canvas.item(id)));
    seen.sort_by_key(|i| i.id());
    seen.dedup_by_key(|i| i.id());

    let mut sizes: Vec<f64> = seen
        .iter()
        .filter_map(|i| match i.shape() {
            Shape::Text { size, .. } => Some(*size),
            _ => None,
        })
        .collect();
    sizes.extend(clusters.iter().filter_map(|c| line_height(canvas, c)));
    let size = median(sizes)
        .map(|s| ((s * 10.0).round() / 10.0).clamp(*TEXT_SIZES.start(), *TEXT_SIZES.end()));
    (most_common(seen.iter().filter_map(|i| colour(i))), size)
}

fn is_stroke(item: &Item) -> bool {
    matches!(item.shape(), Shape::Stroke { .. })
}

fn is_mark(item: &Item) -> bool {
    matches!(item.shape(), Shape::Stroke { .. } | Shape::Text { .. })
}

fn colour(item: &Item) -> Option<Colour> {
    match item.shape() {
        Shape::Stroke { colour, .. } | Shape::Text { colour, .. } => Some(*colour),
        _ => None,
    }
}

/// The colour most often given, the first given on a tie.
fn most_common(colours: impl Iterator<Item = Colour>) -> Option<Colour> {
    let mut counts: Vec<(Colour, usize)> = Vec::new();
    for c in colours {
        match counts.iter_mut().find(|(k, _)| *k == c) {
            Some((_, n)) => *n += 1,
            None => counts.push((c, 1)),
        }
    }
    // `max_by_key` keeps the last of equals; reversed, that is the first.
    counts.into_iter().rev().max_by_key(|&(_, n)| n).map(|(c, _)| c)
}

/// The median height of the lines of writing in a cluster of strokes.
///
/// A line is a run of strokes, top to bottom, each overlapping the line so
/// far by at least half its own height, the way the packet's reading order
/// finds rows; so a descender that reaches the next line does not join the
/// two. Strokes over [`TALL`] times the median stroke height are left
/// out. `None` for a cluster with no strokes.
fn line_height(canvas: &Canvas, strokes: &[ItemId]) -> Option<f64> {
    let spans: Vec<(f64, f64)> = strokes
        .iter()
        .filter_map(|&id| canvas.item(id))
        .map(|i| {
            let b = i.bounds();
            (b.y, b.y + b.h)
        })
        .collect();
    let unit = median(spans.iter().map(|(top, bottom)| bottom - top).collect())?;
    let mut spans: Vec<(f64, f64)> =
        spans.into_iter().filter(|(top, bottom)| bottom - top <= TALL * unit).collect();
    spans.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
    let mut lines: Vec<(f64, f64)> = Vec::new();
    for (top, bottom) in spans {
        match lines.last_mut() {
            Some(line) if line.1.min(bottom) - line.0.max(top) >= (bottom - top) / 2.0 => {
                *line = (line.0.min(top), line.1.max(bottom));
            }
            _ => lines.push((top, bottom)),
        }
    }
    median(lines.into_iter().map(|(top, bottom)| bottom - top).collect())
}

/// The middle value; the mean of the two middle ones for an even count.
fn median(mut v: Vec<f64>) -> Option<f64> {
    v.sort_by(f64::total_cmp);
    let n = v.len();
    match n {
        0 => None,
        _ if n % 2 == 1 => Some(v[n / 2]),
        _ => Some(f64::midpoint(v[n / 2 - 1], v[n / 2])),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_median_of_an_even_count_is_the_mean_of_the_middle_two() {
        assert_eq!(median(vec![]), None);
        assert_eq!(median(vec![3.0, 1.0, 2.0]), Some(2.0));
        assert_eq!(median(vec![4.0, 1.0, 2.0, 10.0]), Some(3.0));
    }

    #[test]
    fn the_most_common_colour_wins_and_a_tie_goes_to_the_first() {
        let (red, blue) = (Colour::rgb(255, 0, 0), Colour::rgb(0, 0, 255));
        assert_eq!(most_common([red, blue, blue].into_iter()), Some(blue));
        assert_eq!(most_common([red, blue].into_iter()), Some(red));
        assert_eq!(most_common(std::iter::empty()), None);
    }
}
