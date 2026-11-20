//! Adds that join a folded box (docs/design/fold.md, "Step in"): what is
//! written in a pane stepped into a folded box goes into it. The pane
//! decides which adds those are ([`Ink::join`]), from a key taken before
//! they were made ([`Ink::mark`]); the outbox sends each with `in`.

use super::{Change, Ink};
use crate::scene::canvas::ItemId;

impl Ink {
    /// The key the next stroke begun, or change queued, takes.
    pub fn mark(&self) -> u64 {
        self.next_key
    }

    /// Each add on `page` of `canvas` queued from key `from` on that
    /// `which` takes joins folded box `inside` once sent. One already
    /// sending stays as it went.
    pub fn join(
        &mut self,
        from: u64,
        (canvas, page): (&str, &str),
        inside: ItemId,
        which: impl Fn(&Change) -> bool,
    ) {
        let joins = |p: &&super::Pending| {
            let c = &p.change;
            let here = c.canvas() == canvas && c.page() == Some(page);
            let adds = matches!(c, Change::Add(_) | Change::Put { .. });
            p.key >= from && p.state == super::State::Waiting && here && adds && which(c)
        };
        let keys: Vec<u64> = self.pending.iter().filter(joins).map(|p| p.key).collect();
        self.joins.extend(keys.into_iter().map(|key| (key, inside)));
    }

    /// The folded box the change sent under `key` joins, if any.
    pub fn joins(&self, key: u64) -> Option<ItemId> {
        self.joins.get(&key).copied()
    }
}
