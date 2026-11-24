//! A folded box's line (docs/design/fold.md, "The packet line"): one `box`
//! line, its box the miniature, with `folded=<n>`, the number of lines its
//! contents would make in a packet of their own, and `new=<strokes>` when
//! it holds new ink of the user's, at any depth. The quoted part is the
//! first line of its first text in reading order, cut as in `page`.

use super::line::{Line, PAGE_TEXT_CHARS, reading_order};
use super::{Index, cluster};
use crate::canvas::{Author, Canvas, Item, ItemId, PageId, Shape};

/// The fields and the quoted part of the line of `folded`, a folded box on
/// `page`. `changed` is the packet's: whether an item is new.
pub(super) fn line(
    canvas: &Canvas,
    page: PageId,
    folded: &Item,
    changed: &dyn Fn(ItemId) -> bool,
) -> (Vec<(&'static str, String)>, Option<String>) {
    let members: Vec<&Item> =
        canvas.contents(folded.id()).into_iter().filter_map(|i| canvas.item(i)).collect();
    let strokes = members.iter().filter(|i| is_stroke(i)).map(|i| i.id());
    let clusters = cluster(canvas, strokes);
    let lines = Index::of(canvas, page, members.clone(), &clusters, changed, None).len();
    let mut fields = vec![("folded", lines.to_string())];
    let held = canvas.held(folded.id()).into_iter().filter_map(|i| canvas.item(i));
    let new = held.filter(|i| is_stroke(i) && *i.author() == Author::User && changed(i.id()));
    match new.count() {
        0 => {}
        n => fields.push(("new", n.to_string())),
    }
    let texts = members.iter().filter_map(|i| match i.shape() {
        Shape::Text { text, .. } => Some(Line {
            n: i.id().get(),
            id: String::new(),
            kind: "text",
            frame: i.bounds(),
            fields: Vec::new(),
            text: Some(text.clone()),
        }),
        _ => None,
    });
    let first = reading_order(texts.collect()).into_iter().next().and_then(|l| l.text);
    (fields, first.map(|t| first_line(&t)))
}

/// The first line of `text`, cut to [`PAGE_TEXT_CHARS`] characters plus
/// `…` when longer: the same cut the `page` section makes, so cutting
/// again there changes nothing.
pub(super) fn first_line(text: &str) -> String {
    let line = text.lines().next().unwrap_or_default();
    if line.chars().count() <= PAGE_TEXT_CHARS {
        return line.to_string();
    }
    let mut cut: String = line.chars().take(PAGE_TEXT_CHARS).collect();
    cut.push('…');
    cut
}

fn is_stroke(item: &Item) -> bool {
    matches!(item.shape(), Shape::Stroke { .. })
}
