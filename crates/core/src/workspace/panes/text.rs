//! The layout's text (see the module's comment in `panes.rs`): writing it,
//! and reading versions 1 and 2. Version 1 has no ids; reading never fails
//! for an id reason, it gives a pane with no id, a 0 or an id already used a
//! fresh one.

use super::{Axis, Kind, Layout, MAX_PANES, MIN_RATIO, Node, Pane, VERSION};

impl Layout {
    /// The layout text: the tree, each pane with its id. The next id is
    /// stored beside it ([`Layout::next`]).
    #[must_use]
    pub fn encode(&self) -> String {
        let mut words = Vec::new();
        self.root.write(&mut words);
        words.join(" ")
    }

    /// The layout [`encode`](Self::encode) wrote, its next id the highest
    /// id + 1. As [`Layout::read`] for this version with no `next`.
    #[must_use]
    pub fn decode(text: &str) -> Option<Self> {
        Self::read(VERSION, text, None)
    }

    /// The layout in `text` of layout version `version` (1 or
    /// [`VERSION`]), with the stored next id `next`. `None` for another
    /// version, text that is not exactly one tree, a share that is not a
    /// finite number, a pane that is not `canvas:<id>` or `ui:<name>`, or
    /// more than [`MAX_PANES`] panes.
    ///
    /// Ids are repaired, not refused: version 1 panes get 1 to n in order;
    /// a version 2 pane with no id, a 0 or the id of a pane before it gets
    /// a fresh one, after every id in the text and `next`; a missing or too
    /// small `next` becomes the highest id + 1.
    #[must_use]
    pub fn read(version: u32, text: &str, next: Option<u32>) -> Option<Self> {
        let ids = match version {
            1 => false,
            VERSION => true,
            _ => return None,
        };
        let mut words = text.split_ascii_whitespace();
        let mut count = 0;
        let mut root = Node::read(&mut words, 0, &mut count, ids)?;
        if words.next().is_some() {
            return None;
        }
        let mut taken = Vec::new();
        root.keep_ids(&mut taken);
        let highest = taken.iter().max().copied().unwrap_or(0);
        let mut next = next.unwrap_or(0).max(highest.saturating_add(1));
        root.give_ids(&mut || {
            let id = next;
            next = next.saturating_add(1);
            id
        });
        Some(Self { root, next })
    }
}

impl Pane {
    /// The pane in a word of the text: `canvas:main`, and in version 2
    /// `canvas:main@3`. A version 2 word whose last `@` is not followed by
    /// a number has no id: its target is the whole.
    fn from_word(word: &str, ids: bool) -> Option<Self> {
        let (kind, rest) = word.split_once(':')?;
        let kind = match kind {
            "canvas" => Kind::Canvas,
            "ui" => Kind::Ui,
            _ => return None,
        };
        let split = ids.then(|| rest.rsplit_once('@')).flatten();
        let numbered = split.and_then(|(target, id)| Some((target, id.parse::<u32>().ok()?)));
        let (target, id) = numbered.unwrap_or((rest, 0));
        Some(Self { id, ..Self::new(kind, target).ok()? })
    }
}

impl Node {
    fn write(&self, words: &mut Vec<String>) {
        match self {
            Self::Pane(pane) => words.push(format!("{}@{}", pane.word(), pane.id)),
            Self::Split { axis, ratio, first, second } => {
                let name = match axis {
                    Axis::Row => "row",
                    Axis::Column => "column",
                };
                words.push(name.to_string());
                words.push(ratio.to_string());
                first.write(words);
                second.write(words);
            }
        }
    }

    /// Keeps each pane's id that is not 0 and not taken by a pane before it,
    /// in order, adding it to `taken`; sets the others to 0.
    fn keep_ids(&mut self, taken: &mut Vec<u32>) {
        match self {
            Self::Pane(pane) if pane.id == 0 || taken.contains(&pane.id) => pane.id = 0,
            Self::Pane(pane) => taken.push(pane.id),
            Self::Split { first, second, .. } => {
                first.keep_ids(taken);
                second.keep_ids(taken);
            }
        }
    }

    /// Gives each pane with id 0 the id `fresh` returns, in order.
    fn give_ids(&mut self, fresh: &mut impl FnMut() -> u32) {
        match self {
            Self::Pane(pane) => {
                if pane.id == 0 {
                    pane.id = fresh();
                }
            }
            Self::Split { first, second, .. } => {
                first.give_ids(fresh);
                second.give_ids(fresh);
            }
        }
    }

    /// One node from `words`, `depth` splits down. `count` counts the panes
    /// read so far; `ids` reads each pane's id (version 2). A tree of at
    /// most `MAX_PANES` panes nests fewer splits than that, so deeper text
    /// is refused before it can use up the stack.
    fn read<'a>(
        words: &mut impl Iterator<Item = &'a str>,
        depth: usize,
        count: &mut usize,
        ids: bool,
    ) -> Option<Self> {
        let word = words.next()?;
        let axis = match word {
            "row" => Axis::Row,
            "column" => Axis::Column,
            _ => {
                *count += 1;
                if *count > MAX_PANES {
                    return None;
                }
                return Pane::from_word(word, ids).map(Self::Pane);
            }
        };
        if depth + 1 >= MAX_PANES {
            return None;
        }
        let ratio: f64 = words.next()?.parse().ok()?;
        if !ratio.is_finite() {
            return None;
        }
        let first = Box::new(Self::read(words, depth + 1, count, ids)?);
        let second = Box::new(Self::read(words, depth + 1, count, ids)?);
        Some(Self::Split { axis, ratio: ratio.clamp(MIN_RATIO, 1.0 - MIN_RATIO), first, second })
    }
}
