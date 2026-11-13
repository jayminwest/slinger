//! The pill's words: how long the agent has worked, what the sling costs,
//! the way to its items, and its answer in lines that fit the panel under
//! the pill.

use std::time::Duration;

use super::{MOST, PAD, TEXT, View};
use crate::layout::{Point, Rect, Size};
use crate::scene::glyphs;

/// "4s", "59s", then "1m", "12m": how long the agent has worked.
pub(super) fn elapsed(time: Duration) -> String {
    let s = time.as_secs();
    if s < 60 { format!("{s}s") } else { format!("{}m", s / 60) }
}

/// " · ~840", " · ~4.2k", " · ~12k", " · ~1.3M": about how many tokens
/// the agent reads for the sling; nothing when the daemon does not count.
pub(super) fn cost(tokens: Option<u64>) -> String {
    let Some(n) = tokens else { return String::new() };
    let tenths = |unit: u64| (n + unit / 20) / (unit / 10);
    let about = match n {
        0..1_000 => n.to_string(),
        1_000..9_950 => format!("{}.{}k", tenths(1_000) / 10, tenths(1_000) % 10),
        9_950..999_500 => format!("{}k", (n + 500) / 1_000),
        _ => format!("{}.{}M", tenths(1_000_000) / 10, tenths(1_000_000) % 10),
    };
    format!(" · ~{about}")
}

/// The answer's lines in a panel `width` wide: its words wrapped as the
/// window measures them, at most [`MOST`] of them, then where the rest is,
/// then how to close it.
pub(super) fn page(answer: &str, width: f64, agent: &str) -> Vec<String> {
    let room = width - 2.0 * PAD;
    let at = Point { x: 0.0, y: 0.0 };
    let fits = |line: &str| glyphs::frame(at, line, TEXT).width <= room;
    let mut lines = wrap(answer, &fits);
    if lines.len() > MOST {
        lines.truncate(MOST - 1);
        lines.push(format!("… the rest: slinger session show {agent}"));
    }
    lines.push("tap to close".to_string());
    lines
}

/// `text` in lines that each `fits`, broken between words where it can
/// be (a word too wide alone is cut, a character at least a line), each of
/// its own lines kept.
pub fn wrap(text: &str, fits: &dyn Fn(&str) -> bool) -> Vec<String> {
    let mut lines = Vec::new();
    for paragraph in text.trim().lines() {
        let mut line = String::new();
        for word in paragraph.split_whitespace() {
            let mut word = word.to_string();
            while !fits(&word) {
                let ends = word.char_indices().map(|(i, _)| i).skip(1);
                let cut = ends.take_while(|&i| fits(&word[..i])).last();
                let Some(cut) = cut.or_else(|| word.char_indices().nth(1).map(|(i, _)| i)) else {
                    break;
                };
                let rest = word.split_off(cut);
                if !line.is_empty() {
                    lines.push(std::mem::take(&mut line));
                }
                lines.push(std::mem::replace(&mut word, rest));
            }
            if !line.is_empty() && !fits(&format!("{line} {word}")) {
                lines.push(std::mem::take(&mut line));
            }
            if !line.is_empty() {
                line.push(' ');
            }
            line.push_str(&word);
        }
        lines.push(line);
    }
    lines
}

/// `n` as a number of points' worth.
pub(super) fn count(n: usize) -> f64 {
    u32::try_from(n).map_or(f64::from(u32::MAX), f64::from)
}

/// " →" (or ←, ↑, ↓): the way from what `view` shows to `away`.
pub(super) fn arrow(away: Rect, view: &View) -> String {
    let seen = view.viewport.visible(Size { width: view.frame.width, height: view.frame.height });
    let dx = (away.x + away.width / 2.0) - (seen.x + seen.width / 2.0);
    let dy = (away.y + away.height / 2.0) - (seen.y + seen.height / 2.0);
    let way = match (dx.abs() >= dy.abs(), dx >= 0.0, dy >= 0.0) {
        (true, true, _) => '→',
        (true, false, _) => '←',
        (false, _, true) => '↓',
        (false, _, false) => '↑',
    };
    format!(" {way}")
}
