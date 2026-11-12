//! Feedback: notes about slinger, left while using it, kept as JSON Lines
//! (docs/service.md, "Feedback"). One [`Entry`] per note, appended, never
//! rewritten; resolving one appends a [`Resolution`] that names the entry's
//! `ts`, which [`stamp`] keeps unique in the file. The format follows Flick's `feedback.jsonl`, so the same `jq`
//! reads both.
//!
//! Pure: the caller owns the file, the clock and the build label. It hands
//! in the file's text and writes the lines this module makes.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::{Error, Result};

/// The file's name, wherever the daemon keeps it.
pub const FILE: &str = "feedback.jsonl";

/// Milliseconds in a second and in a day.
const SECOND: i64 = 1000;
const DAY: i64 = 86_400 * SECOND;

/// What the app was showing when the note was left. Every field is
/// optional and left out of the line when absent; a CLI note has none.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Context {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub canvas: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page: Option<String>,
    /// The pen's tool: `pen`, `lasso`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool: Option<String>,
    /// How many items were selected, when any were.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected: Option<u64>,
}

/// One note: a line of the file. Flat, so `jq -r .text` reads it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    /// When, local time with its UTC offset, to the millisecond:
    /// `2026-11-11T14:52:43.120-08:00` ([`stamp`]). It names the entry:
    /// [`resolve`] takes it. Older files name entries to the second.
    pub ts: String,
    pub text: String,
    /// The build that saved it: a commit (`-dirty` for uncommitted
    /// changes), or `dev`.
    pub build: String,
    #[serde(flatten)]
    pub context: Context,
    /// Set by [`read`] from a later [`Resolution`]; never in the entry's line.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved: Option<Resolved>,
}

/// How an entry was resolved.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Resolved {
    /// When, as [`Entry::ts`].
    pub ts: String,
    /// What resolved it: the issue that tracks it, say.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// A line that resolves every entry whose `ts` is `resolves`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Resolution {
    pub resolves: String,
    #[serde(flatten)]
    pub resolved: Resolved,
}

/// One line of the file, as read.
#[derive(Deserialize)]
#[serde(untagged)]
enum Line {
    Resolution(Resolution),
    Entry(Entry),
}

/// A new entry: `text` trimmed, saved at `ts` by `build`, with `context`
/// (blank fields and a selection of none left out).
///
/// # Errors
///
/// [`Error::Invalid`] when `text` is blank.
pub fn note(text: &str, context: Context, ts: String, build: &str) -> Result<Entry> {
    let text = text.trim();
    if text.is_empty() {
        return Err(Error::Invalid {
            message: "the feedback is empty".into(),
            hint: Some("write what you noticed".into()),
        });
    }
    let given = |field: Option<String>| field.filter(|f| !f.trim().is_empty());
    let context = Context {
        workspace: given(context.workspace),
        canvas: given(context.canvas),
        page: given(context.page),
        tool: given(context.tool),
        selected: context.selected.filter(|&n| n > 0),
    };
    Ok(Entry { ts, text: text.into(), build: build.into(), context, resolved: None })
}

impl Entry {
    /// The entry as one line of the file, with its line break.
    #[must_use]
    pub fn line(&self) -> String {
        line(&Entry { resolved: None, ..self.clone() })
    }
}

impl Resolution {
    /// The resolution as one line of the file, with its line break.
    #[must_use]
    pub fn line(&self) -> String {
        line(self)
    }
}

fn line(value: &impl Serialize) -> String {
    // Strings and options only: serializing cannot fail.
    let mut line = serde_json::to_string(value).unwrap_or_default();
    line.push('\n');
    line
}

/// The newest `limit` entries of the file's `text`, newest first, each with
/// its resolution; only the open ones unless `all`. A line that is not an
/// entry or a resolution is skipped.
#[must_use]
pub fn read(text: &str, limit: usize, all: bool) -> Vec<Entry> {
    let (mut entries, mut resolved) = (Vec::new(), HashMap::new());
    for line in text.lines().filter_map(|l| serde_json::from_str::<Line>(l).ok()) {
        match line {
            Line::Entry(entry) => entries.push(entry),
            Line::Resolution(r) => {
                resolved.insert(r.resolves, r.resolved);
            }
        }
    }
    let entries = entries.into_iter().rev();
    let entries = entries.map(|e| Entry { resolved: resolved.get(&e.ts).cloned(), ..e });
    entries.filter(|e| all || e.resolved.is_none()).take(limit).collect()
}

/// The line that resolves the entry saved at `ts` in the file's `text`, at
/// `at`, with `note` (a blank one left out).
///
/// # Errors
///
/// [`Error::NotFound`] when no entry has that `ts`, and [`Error::Conflict`]
/// when it is resolved already.
pub fn resolve(text: &str, ts: &str, at: String, note: Option<String>) -> Result<Resolution> {
    let entries = read(text, usize::MAX, true);
    let mut matching = entries.iter().filter(|e| e.ts == ts).peekable();
    if matching.peek().is_none() {
        let hint = "pass a ts as 'slinger feedback ls' prints it".to_string();
        return Err(Error::NotFound { what: format!("feedback at {ts}"), hint: Some(hint) });
    }
    if matching.all(|e| e.resolved.is_some()) {
        let hint = "'slinger feedback ls --all' shows how it was resolved".to_string();
        let message = format!("the feedback at {ts} is resolved already");
        return Err(Error::Conflict { message, hint: Some(hint) });
    }
    let note = note.map(|n| n.trim().to_string()).filter(|n| !n.is_empty());
    Ok(Resolution { resolves: ts.into(), resolved: Resolved { ts: at, note } })
}

/// A name for a new entry in the file's `text`, saved at `unix_ms`
/// milliseconds: its [`timestamp`], or the first one after it that no entry
/// in `text` has, so no two entries share a `ts`.
#[must_use]
pub fn stamp(text: &str, unix_ms: i64, offset: i32) -> String {
    let taken: HashSet<String> = read(text, usize::MAX, true).into_iter().map(|e| e.ts).collect();
    // One more millisecond than there are names taken always finds one free.
    (0..=i64::try_from(taken.len()).unwrap_or(i64::MAX))
        .map(|step| timestamp(unix_ms.saturating_add(step), offset))
        .find(|ts| !taken.contains(ts))
        .unwrap_or_else(|| timestamp(unix_ms, offset))
}

/// `unix_ms` milliseconds as local time `offset` seconds east of UTC, in
/// RFC 3339 to the millisecond: `2026-11-11T14:52:43.120-08:00`.
#[must_use]
pub fn timestamp(unix_ms: i64, offset: i32) -> String {
    let local = unix_ms + i64::from(offset) * SECOND;
    let (year, month, day) = civil_from_days(local.div_euclid(DAY));
    let (secs, ms) = (local.rem_euclid(DAY) / SECOND, local.rem_euclid(SECOND));
    let (sign, off) = if offset < 0 { ('-', -offset) } else { ('+', offset) };
    let time = format!("{:02}:{:02}:{:02}.{ms:03}", secs / 3600, secs % 3600 / 60, secs % 60);
    let zone = format!("{sign}{:02}:{:02}", off / 3600, off % 3600 / 60);
    format!("{year:04}-{month:02}-{day:02}T{time}{zone}")
}

/// The UTC offset in seconds that `date +%z` prints (`-0800`, `+0530`);
/// `None` for anything else.
#[must_use]
pub fn offset(text: &str) -> Option<i32> {
    let text = text.trim();
    let (sign, digits) = match text.as_bytes() {
        [b'+', rest @ ..] => (1, rest),
        [b'-', rest @ ..] => (-1, rest),
        _ => return None,
    };
    let &[h1, h2, m1, m2] = digits else { return None };
    let all = [h1, h2, m1, m2];
    if !all.iter().all(u8::is_ascii_digit) {
        return None;
    }
    let [h1, h2, m1, m2] = all.map(|b| i32::from(b - b'0'));
    let (hours, minutes) = (h1 * 10 + h2, m1 * 10 + m2);
    (hours <= 23 && minutes <= 59).then_some(sign * (hours * 3600 + minutes * 60))
}

/// The build label for a commit `sha` and a `dirty` flag (`1` for a build
/// with uncommitted changes): the sha in lowercase, `-dirty` after it when
/// dirty, and `dev` when `sha` is not one.
#[must_use]
pub fn build(sha: Option<&str>, dirty: Option<&str>) -> String {
    let sha = sha.map(str::trim);
    let sha =
        sha.filter(|s| (7..=40).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_hexdigit()));
    match sha {
        Some(sha) if dirty.is_some_and(|d| d.trim() == "1") => {
            format!("{}-dirty", sha.to_lowercase())
        }
        Some(sha) => sha.to_lowercase(),
        None => "dev".into(),
    }
}

/// The civil date (year, month, day) of day `z` since 1970-01-01, in the
/// proleptic Gregorian calendar (Howard Hinnant's algorithm).
fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

#[cfg(test)]
mod tests;
