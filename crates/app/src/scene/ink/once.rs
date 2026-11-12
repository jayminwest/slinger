//! The key each change is applied once under (slinger-1d05ae): the daemon
//! applies a `canvas_apply` sent again under the same key only once
//! (docs/service.md, "Applied once"), so a change whose answer was lost, or
//! a stroke kept on disk that was on its way when the app quit, can be sent
//! again safely. A change's key is this run's name and its own key; a
//! change kept from an earlier run keeps the key it had then ([`Ink::kept`],
//! [`Ink::resume`]).

use std::collections::HashMap;

use super::{Change, Ink, Kept, PENDING, ids};
use crate::scene::canvas::ItemId;

impl Ink {
    /// No ink yet, its changes applied once under keys that start with
    /// `run`, a name no other run of the app shares.
    pub fn named(run: impl Into<String>) -> Self {
        Self { run: run.into(), ..Self::default() }
    }

    /// The key the change under `key` is applied once under.
    pub fn once(&self, key: u64) -> String {
        let kept = self.pending.iter().find(|p| p.key == key).and_then(|p| p.once.clone());
        kept.unwrap_or_else(|| format!("{}.{key}", self.run))
    }

    /// The changes not yet written, oldest first, as they are kept to
    /// survive a quit ([`Ink::unsent`]): each under its key, with the key
    /// it is applied once under.
    pub fn kept(&self) -> Vec<Kept> {
        let after = |key: u64| self.pending.iter().find(|p| p.key == key).and_then(|p| p.after);
        let kept = |(key, change): (u64, &Change)| Kept {
            key: Some(key),
            after: after(key),
            once: Some(self.once(key)),
            change: change.clone(),
        };
        self.unsent().map(kept).collect()
    }

    /// Queues the changes an earlier run kept ([`Ink::kept`]), oldest
    /// first, ahead of any made from now on, each applied once under its
    /// key then. They show under keys of this run, so each id of the ink's
    /// own one of them names (an item another of them adds) is renamed to
    /// the id that item shows under now; one naming an item none of them
    /// adds names an item never made (its add was refused) and is dropped,
    /// as when it is sent ([`ids`](super::ids)). A change goes with the
    /// change it went with then, if that was kept too. Returns each kept
    /// key queued and the key it is queued under now.
    pub fn resume(&mut self, kept: Vec<Kept>) -> HashMap<u64, u64> {
        let mut keys: HashMap<u64, u64> = HashMap::new();
        let mut now: HashMap<ItemId, ItemId> = HashMap::new();
        for Kept { key, after, once, mut change } in kept {
            let renamed = |id| if ids::pending(id) { now.get(&id).copied() } else { Some(id) };
            if !change.rename(renamed) {
                continue;
            }
            let adds = change.adds();
            let new = self.queue(change, after.and_then(|after| keys.get(&after).copied()));
            if let Some(last) = self.pending.last_mut() {
                last.once = once;
            }
            if let Some(old) = key {
                keys.insert(old, new);
                let id = |key: u64, n: u64| ItemId(PENDING.saturating_add(key).saturating_add(n));
                now.extend((0..adds).map(|n| (id(old, n), id(new, n))));
            }
        }
        keys
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Rgb;
    use crate::layout::Point;
    use crate::scene::ink::Stroke;

    fn stroke() -> Stroke {
        Stroke {
            canvas: "main".into(),
            page: "p1".into(),
            colour: Rgb { r: 0, g: 0, b: 0 },
            width: 2.0,
            points: vec![Point { x: 0.0, y: 0.0 }],
        }
    }

    fn kept(once: Option<&str>) -> Kept {
        let once = once.map(str::to_string);
        Kept { key: None, after: None, once, change: Change::Add(stroke()) }
    }

    #[test]
    fn a_change_is_applied_once_under_the_run_s_name_or_the_key_it_was_kept_with() {
        let mut ink = Ink::named("123-456");
        ink.resume(vec![kept(Some("9-9.4")), kept(None)]);
        let new = ink.push_keyed(Change::Add(stroke()));
        let keys: Vec<u64> = ink.unsent().map(|(key, _)| key).collect();
        let [kept, old, _] = keys[..] else { panic!("{keys:?}") };
        assert_eq!(ink.once(kept), "9-9.4");
        assert_eq!(ink.once(old), format!("123-456.{old}"));
        assert_eq!(ink.once(new), format!("123-456.{new}"));
        assert_ne!(ink.once(old), ink.once(new));
        let onces: Vec<_> = ink.kept().into_iter().map(|k| k.once.unwrap()).collect();
        assert_eq!(onces, ["9-9.4".to_string(), ink.once(old), ink.once(new)]);
    }

    /// slinger-e7fc87: an erase, an arrow and a carry of a stroke not yet
    /// written are kept with it, and after a restart still name it, by the
    /// id it shows under then; a change naming only an item never made goes.
    #[test]
    fn kept_changes_name_kept_items_by_their_ids_after_a_restart() {
        use crate::scene::canvas::{Item, Shape};
        use crate::scene::ink::{Answer, Carried, PENDING};
        let (canvas, page) = (String::from("main"), String::from("p1"));
        let remove =
            |ids: Vec<ItemId>| Change::Remove { canvas: canvas.clone(), page: page.clone(), ids };
        let mut quit = Ink::named("a");
        let first = quit.push_keyed(Change::Add(stroke()));
        quit.next();
        quit.answered(first, Answer::Made { seq: 1, ids: vec![ItemId(7)] });
        let second = quit.push_keyed(Change::Add(stroke()));
        let drawn = ItemId(PENDING + second);
        quit.push(remove(vec![ItemId(7), drawn]));
        let bound = [Some(drawn), Some(ItemId(7))];
        let at = Point { x: 0.0, y: 0.0 };
        let arrow = Shape::Arrow { from: at, to: at, width: 2.0, bound };
        let colour = Rgb { r: 0, g: 0, b: 0 };
        quit.push(Change::Put { canvas: canvas.clone(), page: page.clone(), colour, shape: arrow });
        quit.push(remove(vec![ItemId(PENDING + 99)]));
        let item = Item { id: drawn, colour, shape: Shape::Stroke { points: vec![], width: 2.0 } };
        let items = vec![Carried { item, object: serde_json::json!({"kind": "stroke"}) }];
        let to = Change::Carry { canvas: "other".into(), page: "p1".into(), items, gone: vec![] };
        quit.carry(to, remove(vec![drawn]));
        let kept = quit.kept();
        assert_eq!(kept.len(), 6, "all but the stroke written");
        assert_eq!(kept[5].after, kept[4].key, "the carry's removal goes with its add");

        let mut ink = Ink::named("b");
        ink.resume(kept);
        let mut sent = Vec::new();
        let mut made = 20;
        while let Some((key, change)) = ink.next() {
            made += 1;
            let adds = matches!(change, Change::Add(_) | Change::Put { .. } | Change::Carry { .. });
            sent.push((ink.once(key), change));
            let answer = Answer::Made { seq: made, ids: vec![ItemId(made)] };
            ink.answered(key, if adds { answer } else { Answer::Applied(made) });
        }
        let onces: Vec<&str> = sent.iter().map(|(once, _)| once.as_str()).collect();
        assert_eq!(onces, ["a.1", "a.2", "a.3", "a.5", "a.6"], "the removal of nothing gone");
        let changes: Vec<&Change> = sent.iter().map(|(_, change)| change).collect();
        assert_eq!(changes[1], &remove(vec![ItemId(7), ItemId(21)]), "the stroke as written");
        let Change::Put { shape: Shape::Arrow { bound, .. }, .. } = changes[2] else { panic!() };
        assert_eq!(*bound, [Some(ItemId(21)), Some(ItemId(7))]);
        assert_eq!(changes[4], &remove(vec![ItemId(21)]));
    }
}
