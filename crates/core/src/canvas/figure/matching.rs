//! Replace by match (docs/design/figures.md, "Replace by match"): which of a
//! figure's items a new drawing keeps, changes, adds and takes away. Each
//! item has a key (its part, or none, and its kind) and an ordinal among
//! the items of that key: the old ones in id order, the new ones in the
//! order the script emitted them. The same key and ordinal on both sides is
//! one item, kept under its id; so a script that reorders its calls within
//! one part and kind changes contents, never ids, and across parts and kinds
//! it changes nothing.
//!
//! Pure: keys in, pairs out. Whether a kept item changed is the caller's
//! to say.

use std::collections::HashMap;
use std::hash::Hash;

/// What became of one item: kept (the old item's index and the new one's),
/// new, or gone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Pair {
    Both(usize, usize),
    New(usize),
    Gone(usize),
}

/// The pairs of `old` and `new` keys, each index of both sides in exactly
/// one: first each new key in order (`Both` or `New`), then each old key
/// no new one took, in order (`Gone`).
pub(crate) fn pair<K: Eq + Hash>(old: &[K], new: &[K]) -> Vec<Pair> {
    let mut olds: HashMap<(&K, usize), usize> = HashMap::with_capacity(old.len());
    let mut seen: HashMap<&K, usize> = HashMap::new();
    for (i, key) in old.iter().enumerate() {
        let n = seen.entry(key).or_default();
        olds.insert((key, *n), i);
        *n += 1;
    }
    seen.clear();
    let mut kept = vec![false; old.len()];
    let mut out = Vec::with_capacity(old.len().max(new.len()));
    for (j, key) in new.iter().enumerate() {
        let n = seen.entry(key).or_default();
        match olds.get(&(key, *n)) {
            Some(&i) => {
                kept[i] = true;
                out.push(Pair::Both(i, j));
            }
            None => out.push(Pair::New(j)),
        }
        *n += 1;
    }
    out.extend(kept.iter().enumerate().filter(|(_, k)| !**k).map(|(i, _)| Pair::Gone(i)));
    out
}

#[cfg(test)]
mod tests {
    use super::Pair::{Both, Gone, New};
    use super::*;

    #[test]
    fn the_same_keys_pair_one_to_one_in_order() {
        let keys = ["a", "b", "a", "c"];
        assert_eq!(pair(&keys, &keys), vec![Both(0, 0), Both(1, 1), Both(2, 2), Both(3, 3)]);
    }

    #[test]
    fn items_of_one_key_pair_by_ordinal_whatever_lies_between() {
        // The script moved its "b" call first: the two "a"s still pair in order.
        let (old, new) = (["a", "b", "a"], ["b", "a", "a"]);
        assert_eq!(pair(&old, &new), vec![Both(1, 0), Both(0, 1), Both(2, 2)]);
    }

    #[test]
    fn more_new_items_add_and_fewer_take_the_last_away() {
        assert_eq!(pair(&["a"], &["a", "a", "b"]), vec![Both(0, 0), New(1), New(2)]);
        assert_eq!(pair(&["a", "b", "a"], &["a"]), vec![Both(0, 0), Gone(1), Gone(2)]);
    }

    #[test]
    fn nothing_on_one_side_is_all_new_or_all_gone() {
        let none: [&str; 0] = [];
        assert_eq!(pair(&none, &["a", "b"]), vec![New(0), New(1)]);
        assert_eq!(pair(&["a", "b"], &none), vec![Gone(0), Gone(1)]);
    }
}
