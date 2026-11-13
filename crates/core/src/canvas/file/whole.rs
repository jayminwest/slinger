//! A whole canvas as the file that builds it: for a canvas made somewhere
//! else (an import) that becomes a stored canvas in one write.
//!
//! The lines rebuild the canvas with its ids, makers and times: the first
//! page renamed, the other pages added, then every item added in id order. An
//! arrow is added with no item ends, and a `set` after the last item points it
//! at its items, so no line points at an item a later line makes.

use super::json;
use super::{Change, Header, Line, snap_shape};
use crate::canvas::{Author, Canvas, End, FIRST_PAGE, PageId, Shape, Step, Time};

/// The bytes of a new canvas file `id` that replays into `canvas`. Page lines
/// are made by `user` at `at`; each item's line keeps its own maker and time.
/// The pages must have rising ids in page order, as a canvas built only by
/// adding pages has.
pub(super) fn encode(id: &str, canvas: &Canvas, at: Time) -> String {
    let mut out = Header::new(id).encode();
    let mut n = 1;
    let mut push = |by: &Author, at: Time, change: Change| {
        out.push_str(&Line::Op { n, by: by.clone(), at, change, step: Step::None }.encode());
        n += 1;
    };
    let user = Author::User;
    let mut first_kept = false;
    for page in canvas.pages() {
        let name = page.name().to_owned();
        if page.id().get() == 1 {
            first_kept = true;
            if name != FIRST_PAGE {
                push(&user, at, Change::PageRename { page: page.id(), name });
            }
        } else {
            push(&user, at, Change::PageAdd { page: page.id(), name });
        }
    }
    if !first_kept {
        push(&user, at, Change::PageRemove { page: PageId::new(1) });
    }
    let mut items: Vec<_> = canvas.pages().flat_map(|p| canvas.items(p.id())).collect();
    items.sort_by_key(|i| i.id());
    let mut bound = Vec::new();
    for item in items {
        let shape = match item.shape() {
            Shape::Arrow { from, to, colour } if from.item.is_some() || to.item.is_some() => {
                bound.push(item);
                Shape::Arrow { from: End::free(from.at), to: End::free(to.at), colour: *colour }
            }
            other => other.clone(),
        };
        let change = Change::Add { item: item.id(), page: item.page(), shape: snap_shape(&shape) };
        push(item.author(), item.time(), change);
    }
    for item in bound {
        let fields = json::fields(&snap_shape(item.shape()));
        push(item.author(), item.time(), Change::Set { item: item.id(), fields });
    }
    out
}
