//! The pill's words: how long the agent has worked, and its answer in
//! lines that fit the panel under the pill.

use std::time::Duration;

use super::{CHAR, MOST, PAD};

/// "4s", "59s", then "1m", "12m": how long the agent has worked.
pub(super) fn elapsed(time: Duration) -> String {
    let s = time.as_secs();
    if s < 60 { format!("{s}s") } else { format!("{}m", s / 60) }
}

/// The answer's lines in a panel `width` wide: its words wrapped, at most
/// [`MOST`] of them, then where the rest is, then how to close it.
pub(super) fn page(answer: &str, width: f64, agent: &str) -> Vec<String> {
    let room = ((width - 2.0 * PAD) / CHAR).floor().max(8.0);
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a small, positive width"
    )]
    let room = room as usize;
    let mut lines = wrap(answer, room);
    if lines.len() > MOST {
        lines.truncate(MOST - 1);
        lines.push(format!("… the rest: slinger session show {agent}"));
    }
    lines.push("tap to close".to_string());
    lines
}

/// `text` in lines of at most `room` characters, broken between words
/// where it can be, each of its own lines kept.
pub fn wrap(text: &str, room: usize) -> Vec<String> {
    let mut lines = Vec::new();
    for paragraph in text.trim().lines() {
        let mut line = String::new();
        for word in paragraph.split_whitespace() {
            let mut word = word.to_string();
            while word.chars().count() > room {
                let rest =
                    word.split_off(word.char_indices().nth(room).map_or(word.len(), |(i, _)| i));
                if !line.is_empty() {
                    lines.push(std::mem::take(&mut line));
                }
                lines.push(std::mem::replace(&mut word, rest));
            }
            let fits = line.chars().count() + 1 + word.chars().count() <= room;
            if !line.is_empty() && !fits {
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
