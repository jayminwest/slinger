//! The markdown a canvas text not being edited is drawn with
//! (docs/design/writing-column.md, 4A): a small subset styled in place, on
//! the same cells the text takes as typed, so a word is in the same cell
//! edited or not. Pure: one stored line in, the look of each of its cells
//! out ([`line`]); [`super::written`] lays the looks on the rows.
//!
//! - `#` headings: bold, the `#`s dimmed.
//! - `*em*`, `_em_` italic, `**strong**` bold, `***both***`: marks dimmed.
//! - `- `, `* `, `+ `, `1. ` list marks dimmed; `- [ ]` and `- [x]` a
//!   ☐ or ☑ in the middle of the three cells.
//! - `> ` quotes: a bar in the `>`'s cell.
//! - `code spans` on a tint, their backticks dimmed; a fenced code block's
//!   lines plain on a tint, its fences dimmed.
//!
//! No mark is removed and none takes another cell count. Everything else
//! (tables, images, links) is drawn as typed. Hard line breaks are kept:
//! each stored line starts a row (2A).

/// What a cell shows in place of its character.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Shown {
    /// Its character.
    #[default]
    As,
    /// Nothing: a checkbox's bracket.
    Blank,
    /// A bar down its left part: a quote's `>`.
    Bar,
    /// A checkbox, ticked or not, in place of its middle character.
    Check(bool),
}

/// How one cell of a text is drawn.
#[expect(clippy::struct_excessive_bools, reason = "one bool per way a cell is styled")]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Look {
    pub bold: bool,
    pub italic: bool,
    /// A mark: in the ink faded toward the paper.
    pub dim: bool,
    /// Code: on a tint.
    pub code: bool,
    pub shown: Shown,
}

impl Look {
    const DIM: Look = Look { bold: false, italic: false, dim: true, code: false, shown: Shown::As };
}

/// Whether a line opens or closes a fenced code block: three or more
/// backticks or tildes after at most three spaces.
fn fence(chars: &[char]) -> bool {
    let lead = chars.iter().take_while(|&&c| c == ' ').count();
    let rest = &chars[lead..];
    lead <= 3 && rest.len() >= 3 && (rest[..3] == ['`'; 3] || rest[..3] == ['~'; 3])
}

/// The look of each character of `chars`, one stored line of a text,
/// and whether a fenced code block is open after it; `fenced`: whether
/// one was before it.
pub fn line(chars: &[char], fenced: bool) -> (Vec<Look>, bool) {
    let mut looks = vec![Look::default(); chars.len()];
    if fence(chars) {
        looks.fill(Look { code: true, ..Look::DIM });
        return (looks, !fenced);
    }
    if fenced {
        looks.fill(Look { code: true, ..Look::default() });
        return (looks, true);
    }
    let mut i = chars.iter().take_while(|&&c| c == ' ').count().min(3);
    // Quotes, nested.
    while chars.get(i) == Some(&'>') {
        looks[i].shown = Shown::Bar;
        looks[i].dim = true;
        i += 1;
        if chars.get(i) == Some(&' ') {
            i += 1;
        }
    }
    let base = heading(chars, i, &mut looks)
        .map(|after| {
            i = after;
            Look { bold: true, ..Look::default() }
        })
        .unwrap_or_else(|| {
            i = list(chars, i, &mut looks);
            Look::default()
        });
    inline(chars, (i, chars.len()), base, &mut looks);
    (looks, false)
}

/// A heading's `#`s from `i`: dimmed, and where its words start; `None`
/// when the line is no heading.
fn heading(chars: &[char], i: usize, looks: &mut [Look]) -> Option<usize> {
    let hashes = chars[i..].iter().take_while(|&&c| c == '#').count();
    let after = i + hashes;
    if !(1..=6).contains(&hashes) || chars.get(after).is_some_and(|&c| c != ' ') {
        return None;
    }
    looks[i..after].fill(Look::DIM);
    Some(after)
}

/// A list's mark from `i`, and a checkbox after it: dimmed, and where the
/// item's words start (`i` when the line is no list item).
fn list(chars: &[char], i: usize, looks: &mut [Look]) -> usize {
    let digits = chars[i..].iter().take_while(|c| c.is_ascii_digit()).count();
    let mark = match chars.get(i) {
        Some('-' | '*' | '+') => 1,
        _ if (1..=9).contains(&digits) && matches!(chars.get(i + digits), Some('.' | ')')) => {
            digits + 1
        }
        _ => return i,
    };
    if chars.get(i + mark).is_some_and(|&c| c != ' ') {
        return i;
    }
    looks[i..i + mark].fill(Look::DIM);
    let at = i + mark + 1;
    let ticked = match chars.get(at..at + 3) {
        Some(['[', ' ', ']']) => false,
        Some(['[', 'x' | 'X', ']']) => true,
        _ => return at.min(chars.len()),
    };
    if chars.get(at + 3).is_some_and(|&c| c != ' ') {
        return at;
    }
    looks[at] = Look { shown: Shown::Blank, ..Look::DIM };
    looks[at + 1] = Look { shown: Shown::Check(ticked), ..Look::default() };
    looks[at + 2] = Look { shown: Shown::Blank, ..Look::DIM };
    at + 3
}

/// Code spans and emphasis in `chars` from `from` to `to`, on `base`.
fn inline(chars: &[char], (from, to): (usize, usize), base: Look, looks: &mut [Look]) {
    let mut i = from;
    while i < to {
        looks[i] = Look { shown: looks[i].shown, ..base };
        match chars[i] {
            '\\' if i + 1 < to && chars[i + 1].is_ascii_punctuation() => {
                looks[i] = Look { dim: true, ..base };
                looks[i + 1] = base;
                i += 2;
            }
            '`' => i = code(chars, (i, to), base, looks),
            c @ ('*' | '_') => i = emphasis(chars, (i, to), c, base, looks),
            _ => i += 1,
        }
    }
}

/// How many `c`s in a row from `i`, not past `to`.
fn run(chars: &[char], i: usize, to: usize, c: char) -> usize {
    chars[i..to].iter().take_while(|&&d| d == c).count()
}

/// A code span from the backticks at `at`: up to the next run of as
/// many. Returns where to go on.
fn code(chars: &[char], (at, to): (usize, usize), base: Look, looks: &mut [Look]) -> usize {
    let n = run(chars, at, to, '`');
    let mut j = at + n;
    while j < to {
        let m = run(chars, j, to, '`');
        if m == n {
            let tick = Look { dim: true, code: true, ..base };
            looks[at..at + n].fill(tick);
            looks[at + n..j].fill(Look { code: true, ..base });
            looks[j..j + n].fill(tick);
            return j + n;
        }
        j += m.max(1);
    }
    looks[at..at + n].fill(base);
    at + n
}

/// Emphasis from the run of `c` at `at` (one: italic, two: bold, three:
/// both) to the next such run that may close it. Returns where to go on.
fn emphasis(
    chars: &[char],
    (at, to): (usize, usize),
    c: char,
    base: Look,
    looks: &mut [Look],
) -> usize {
    let n = run(chars, at, to, c);
    let word = |k: usize| chars.get(k).is_some_and(|c| c.is_alphanumeric());
    let space = |k: usize| chars.get(k).is_none_or(|c| c.is_whitespace());
    // An opener has a character after it; an underscore none before it.
    let opens = n <= 3 && !space(at + n) && !(c == '_' && at > 0 && word(at - 1));
    let close = (at + n + 1..to).find(|&j| {
        run(chars, j, to, c) == n
            && (j == 0 || chars[j - 1] != c)
            && !space(j - 1)
            && !(c == '_' && word(j + n))
    });
    let Some(j) = close.filter(|_| opens) else {
        looks[at..at + n].fill(base);
        return at + n;
    };
    let inner = Look { bold: base.bold || n >= 2, italic: base.italic || n != 2, ..base };
    looks[at..at + n].fill(Look { dim: true, ..base });
    inline(chars, (at + n, j), inner, looks);
    looks[j..j + n].fill(Look { dim: true, ..base });
    j + n
}

#[cfg(test)]
mod tests;
