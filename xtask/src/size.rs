//! check:size: `.rs` files stay within `limit` lines unless budgeted, and budgets
//! only shrink. Config: `budgets/file-size-budgets.json`.

use std::collections::BTreeMap;
use std::path::Path;

use crate::files;

const BUDGETS: &str = "budgets/file-size-budgets.json";

pub fn check(root: &Path) -> Result<String, Vec<String>> {
    let config = files::read_json(root, BUDGETS)?;
    let limit = config["limit"]
        .as_u64()
        .ok_or_else(|| vec![format!("{BUDGETS}: limit must be a number")])?;
    let budgets: BTreeMap<String, u64> = config["budgets"]
        .as_object()
        .map(|m| m.iter().filter_map(|(k, v)| Some((k.clone(), v.as_u64()?))).collect())
        .unwrap_or_default();
    let sizes: Vec<(String, u64)> = files::under(root, &["crates", "xtask"], &["rs"])
        .into_iter()
        .map(|f| {
            let lines = files::read(root, &f).map_or(0, |s| s.lines().count() as u64);
            (f, lines)
        })
        .collect();
    let errors = evaluate(&sizes, limit, &budgets);
    if errors.is_empty() {
        Ok(format!("check:size: {} files within budget", sizes.len()))
    } else {
        Err(errors)
    }
}

pub fn evaluate(
    sizes: &[(String, u64)],
    limit: u64,
    budgets: &BTreeMap<String, u64>,
) -> Vec<String> {
    let mut errors = Vec::new();
    for (file, lines) in sizes {
        match budgets.get(file) {
            Some(budget) if lines > budget => {
                errors.push(format!(
                    "{file} has {lines} lines, over its budget of {budget}; split it"
                ));
            }
            None if *lines > limit => {
                errors.push(format!(
                    "{file} has {lines} lines, over the {limit}-line limit; split it"
                ));
            }
            _ => {}
        }
    }
    for file in budgets.keys() {
        let current = sizes.iter().find(|(f, _)| f == file).map(|(_, n)| *n);
        if current.is_none_or(|n| n <= limit) {
            errors.push(format!(
                "stale entry {file} in {BUDGETS} (gone or within the limit); delete it"
            ));
        }
    }
    errors
}

#[cfg(test)]
mod tests {
    use super::*;

    const NONE: [String; 0] = [];

    fn sizes(entries: &[(&str, u64)]) -> Vec<(String, u64)> {
        entries.iter().map(|(f, n)| ((*f).to_string(), *n)).collect()
    }

    #[test]
    fn passes_files_within_the_limit() {
        assert_eq!(evaluate(&sizes(&[("a.rs", 500)]), 500, &BTreeMap::new()), NONE);
    }

    #[test]
    fn fails_an_unbudgeted_file_over_the_limit() {
        let errors = evaluate(&sizes(&[("a.rs", 501)]), 500, &BTreeMap::new());
        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("500-line limit"));
    }

    #[test]
    fn holds_a_budgeted_file_to_its_budget() {
        let budgets = BTreeMap::from([("a.rs".to_string(), 600)]);
        assert_eq!(evaluate(&sizes(&[("a.rs", 600)]), 500, &budgets), NONE);
        assert!(evaluate(&sizes(&[("a.rs", 601)]), 500, &budgets)[0].contains("budget of 600"));
    }

    #[test]
    fn fails_stale_budget_entries() {
        let budgets = BTreeMap::from([("gone.rs".to_string(), 600), ("small.rs".to_string(), 600)]);
        let errors = evaluate(&sizes(&[("small.rs", 10)]), 500, &budgets);
        assert_eq!(errors.len(), 2);
        assert!(errors.iter().all(|e| e.contains("stale")));
    }
}
