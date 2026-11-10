//! check:debt: every debt marker cites a tracker id, an issue or a URL on the
//! same line, or is allowlisted by `path:line` with a reason. Stale allowlist
//! entries fail. Config: `budgets/debt-allowlist.json`.

use std::path::Path;

use crate::files;

const ALLOWLIST: &str = "budgets/debt-allowlist.json";
const TRACKER: &str = "slinger";
const MARKERS: &[&str] = &["TODO", "FIXME", "HACK", "XXX"];
/// This file names the markers itself.
const SELF: &str = "xtask/src/debt.rs";

pub fn check(root: &Path) -> Result<String, Vec<String>> {
    let config = files::read_json(root, ALLOWLIST)?;
    let mut errors = Vec::new();
    let mut allowed = Vec::new();
    for entry in config["allow"].as_array().into_iter().flatten() {
        match (entry["at"].as_str(), entry["why"].as_str()) {
            (Some(at), Some(why)) if !at.is_empty() && !why.is_empty() => {
                allowed.push(at.to_string());
            }
            _ => errors.push(format!("{ALLOWLIST}: every entry needs a non-empty `at` and `why`")),
        }
    }
    let mut used = Vec::new();
    let scanned = files::under(root, &["crates", "xtask", "scripts"], &["rs", "sh", "toml"]);
    for file in scanned.iter().filter(|f| *f != SELF) {
        let text = files::read(root, file)?;
        for (i, line) in text.lines().enumerate() {
            if !has_marker(line) || is_cited(line, TRACKER) {
                continue;
            }
            let at = format!("{file}:{}", i + 1);
            if allowed.contains(&at) {
                used.push(at);
            } else {
                errors.push(format!("{at}: {}  [cite {TRACKER}-xxxx, #N or a URL]", line.trim()));
            }
        }
    }
    for at in allowed.iter().filter(|a| !used.contains(a)) {
        errors
            .push(format!("stale entry {at} in {ALLOWLIST} (no uncited marker there); delete it"));
    }
    if errors.is_empty() {
        Ok(format!("check:debt: {} files, no uncited markers", scanned.len()))
    } else {
        Err(errors)
    }
}

/// A marker as a whole word: `TODO` counts, `TODOS` and `MY_TODO` do not.
pub fn has_marker(line: &str) -> bool {
    let is_word = |c: char| c.is_ascii_alphanumeric() || c == '_';
    MARKERS.iter().any(|m| {
        line.match_indices(m).any(|(i, _)| {
            let before = line[..i].chars().next_back();
            let after = line[i + m.len()..].chars().next();
            !before.is_some_and(is_word) && !after.is_some_and(is_word)
        })
    })
}

/// Cites `<tracker>-xxxx` (4+ alphanumerics), `#<digits>` or an http(s) URL.
pub fn is_cited(line: &str, tracker: &str) -> bool {
    if line.contains("http://") || line.contains("https://") {
        return true;
    }
    let issue = line
        .match_indices('#')
        .any(|(i, _)| line[i + 1..].starts_with(|c: char| c.is_ascii_digit()));
    let prefix = format!("{tracker}-");
    let seed = line.match_indices(&prefix).any(|(i, _)| {
        line[i + prefix.len()..].chars().take_while(char::is_ascii_alphanumeric).count() >= 4
    });
    issue || seed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_whole_word_markers_only() {
        assert!(has_marker("// TODO: later"));
        assert!(has_marker("# FIXME(x)"));
        assert!(!has_marker("let todos = 1; // TODOS"));
        assert!(!has_marker("MY_TODO_LIST"));
    }

    #[test]
    fn accepts_tracker_issue_and_url_citations() {
        assert!(is_cited("// TODO(acme-1a2b): later", "acme"));
        assert!(is_cited("// TODO: see #42", "acme"));
        assert!(is_cited("// TODO: https://example.com/x", "acme"));
    }

    #[test]
    fn rejects_placeholder_and_foreign_citations() {
        assert!(!is_cited("// TODO(acme-12): too short", "acme"));
        assert!(!is_cited("// TODO(other-1a2b): wrong tracker", "acme"));
        assert!(!is_cited("// TODO: # not an issue", "acme"));
    }
}
