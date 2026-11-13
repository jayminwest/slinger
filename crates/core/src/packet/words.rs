//! The PDF's words a cluster of ink marks (docs/design/pdf.md, "Words"): a
//! word belongs to a cluster when its centre lies inside the cluster's box
//! grown on every side by half the word's height. A circle or a strike
//! takes the words in it, an underline the words over it and not the line
//! below.

use crate::canvas::Rect;
use crate::render::{Backdrop, Word};

/// The most characters of words one ink line carries, before `…`.
pub const MAX_WORDS_CHARS: usize = 2000;

/// The words near `ink` (a cluster's box) in `under`, in backdrop order then
/// file order, each text and box once, joined by single spaces and cut to
/// [`MAX_WORDS_CHARS`] plus `…`. `None` when there are none.
pub(super) fn near(ink: Rect, under: &[(&Backdrop, Rect)]) -> Option<String> {
    let mut taken: Vec<&Word> = Vec::new();
    for word in under.iter().flat_map(|(b, _)| &b.words) {
        let f = word.frame;
        let pad = f.h / 2.0;
        let (cx, cy) = (f.x + f.w / 2.0, f.y + f.h / 2.0);
        let inside = cx >= ink.x - pad
            && cx <= ink.x + ink.w + pad
            && cy >= ink.y - pad
            && cy <= ink.y + ink.h + pad;
        if inside && !word.text.is_empty() && !taken.contains(&word) {
            taken.push(word);
        }
    }
    if taken.is_empty() {
        return None;
    }
    let all = taken.iter().map(|w| w.text.as_str()).collect::<Vec<_>>().join(" ");
    if all.chars().count() <= MAX_WORDS_CHARS {
        return Some(all);
    }
    let mut cut: String = all.chars().take(MAX_WORDS_CHARS).collect();
    cut.push('…');
    Some(cut)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canvas::ItemId;

    fn word(text: &str, x: f64, y: f64) -> Word {
        Word { text: text.into(), frame: Rect::new(x, y, 40.0, 10.0) }
    }

    fn backdrop(words: Vec<Word>) -> Backdrop {
        let area = Rect::new(0.0, 0.0, 10.0, 10.0);
        Backdrop::new(ItemId::new(1), area, vec![255; 4 * 4 * 3], words).unwrap()
    }

    fn near_of(ink: Rect, backdrops: &[Backdrop]) -> Option<String> {
        let under: Vec<_> = backdrops.iter().map(|b| (b, b.area)).collect();
        near(ink, &under)
    }

    #[test]
    fn a_circle_takes_the_words_in_it_and_not_those_beside() {
        let line = vec![word("before", 0.0, 100.0), word("inside", 60.0, 100.0)];
        let b = backdrop([line, vec![word("after", 200.0, 100.0)]].concat());
        let circle = Rect::new(50.0, 90.0, 60.0, 30.0);
        assert_eq!(near_of(circle, &[b]).as_deref(), Some("inside"));
    }

    #[test]
    fn an_underline_takes_the_words_over_it_and_not_the_line_below() {
        let words = vec![word("over", 0.0, 100.0), word("below", 0.0, 114.0)];
        // A flat line just under the first word: its centre is 5 above,
        // the next line's 9 below; half a word's height is 5.
        let underline = Rect::new(0.0, 110.0, 40.0, 0.0);
        assert_eq!(near_of(underline, &[backdrop(words)]).as_deref(), Some("over"));
    }

    #[test]
    fn a_word_in_two_backdrops_is_taken_once_in_backdrop_order() {
        let (a, b) = (word("one", 0.0, 0.0), word("two", 10.0, 0.0));
        let backdrops = [backdrop(vec![b.clone(), a.clone()]), backdrop(vec![a, b])];
        let ink = Rect::new(0.0, 0.0, 60.0, 10.0);
        assert_eq!(near_of(ink, &backdrops).as_deref(), Some("two one"));
        assert_eq!(near_of(Rect::new(500.0, 500.0, 1.0, 1.0), &backdrops), None);
    }

    #[test]
    fn words_past_the_limit_are_cut_with_an_ellipsis() {
        let words = (0..500).map(|n| word(&format!("w{n:07}"), 0.0, 0.0)).collect();
        let ink = Rect::new(0.0, 0.0, 100.0, 10.0);
        let said = near_of(ink, &[backdrop(words)]).unwrap();
        assert_eq!(said.chars().count(), MAX_WORDS_CHARS + 1);
        assert!(said.ends_with('…') && said.starts_with("w0000000 w0000001"), "{said}");
    }
}
