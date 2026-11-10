//! check:coverage: runs the workspace tests under `cargo llvm-cov` and holds
//! line coverage at or above the floors in `budgets/coverage-budgets.json`.
//! xtask itself is tested but not measured.

use std::path::Path;

use crate::files;

const BUDGETS: &str = "budgets/coverage-budgets.json";
const REPORT: &str = "target/llvm-cov-summary.json";

/// Lines covered and lines total for one file.
pub struct FileLines {
    pub file: String,
    pub covered: u64,
    pub count: u64,
}

pub fn check(root: &Path) -> Result<String, Vec<String>> {
    let budgets = files::read_json(root, BUDGETS)?;
    crate::run(
        root,
        "cargo",
        &[
            "llvm-cov",
            "--workspace",
            "--locked",
            "--ignore-filename-regex",
            "xtask/",
            "--json",
            "--summary-only",
            "--output-path",
            REPORT,
        ],
    )?;
    let report = files::read_json(root, REPORT)?;
    let rows: Vec<FileLines> = report["data"][0]["files"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|f| FileLines {
            file: files::relative(root, Path::new(f["filename"].as_str().unwrap_or_default())),
            covered: f["summary"]["lines"]["covered"].as_u64().unwrap_or(0),
            count: f["summary"]["lines"]["count"].as_u64().unwrap_or(0),
        })
        .collect();
    let mut floors: Vec<(String, f64)> =
        vec![("total".into(), budgets["total"].as_f64().unwrap_or(0.0))];
    if let Some(map) = budgets["floors"].as_object() {
        floors.extend(map.iter().map(|(k, v)| (k.clone(), v.as_f64().unwrap_or(0.0))));
    }
    let (report, errors) = evaluate(&rows, &floors);
    if errors.is_empty() { Ok(report.join("\n")) } else { Err(errors) }
}

/// Percent of lines covered across `rows`; an empty scope counts as fully covered.
#[expect(clippy::cast_precision_loss, reason = "line counts are far below 2^52")]
pub fn percent(rows: &[&FileLines]) -> f64 {
    let covered: u64 = rows.iter().map(|r| r.covered).sum();
    let count: u64 = rows.iter().map(|r| r.count).sum();
    if count == 0 { 100.0 } else { covered as f64 * 100.0 / count as f64 }
}

/// Checks each `(scope, floor)`. A scope is `total`, a file, or a directory ending
/// in `/`. Returns the per-scope report lines and the errors.
pub fn evaluate(rows: &[FileLines], floors: &[(String, f64)]) -> (Vec<String>, Vec<String>) {
    let mut report = Vec::new();
    let mut errors = Vec::new();
    for (scope, floor) in floors {
        let matched: Vec<&FileLines> = rows
            .iter()
            .filter(|r| match scope.as_str() {
                "total" => true,
                s if s.ends_with('/') => r.file.starts_with(s),
                s => r.file == s,
            })
            .collect();
        if matched.is_empty() {
            errors.push(format!(
                "coverage scope {scope} in {BUDGETS} matches no file; delete or fix it"
            ));
            continue;
        }
        let pct = percent(&matched);
        if pct + 1e-9 < *floor {
            errors.push(format!("{scope} line coverage {pct:.2}% is below its {floor}% floor"));
        } else {
            report.push(format!("{scope}: {pct:.2}% (floor {floor}%)"));
        }
    }
    (report, errors)
}

#[cfg(test)]
mod tests {
    use super::*;

    const NONE: [String; 0] = [];

    fn rows() -> Vec<FileLines> {
        vec![
            FileLines { file: "crates/core/src/lib.rs".into(), covered: 9, count: 10 },
            FileLines { file: "crates/cli/src/main.rs".into(), covered: 1, count: 10 },
        ]
    }

    fn floor(scope: &str, pct: f64) -> Vec<(String, f64)> {
        vec![(scope.to_string(), pct)]
    }

    #[test]
    fn checks_the_total() {
        assert_eq!(evaluate(&rows(), &floor("total", 50.0)).1, NONE);
        assert_eq!(evaluate(&rows(), &floor("total", 50.1)).1.len(), 1);
    }

    #[test]
    fn checks_directory_and_file_scopes() {
        assert_eq!(evaluate(&rows(), &floor("crates/core/", 90.0)).1, NONE);
        assert_eq!(evaluate(&rows(), &floor("crates/cli/src/main.rs", 10.0)).1, NONE);
        assert_eq!(evaluate(&rows(), &floor("crates/cli/", 11.0)).1.len(), 1);
    }

    #[test]
    fn fails_a_scope_that_matches_nothing() {
        let (_, errors) = evaluate(&rows(), &floor("crates/gone/", 0.0));
        assert!(errors[0].contains("matches no file"));
    }

    #[test]
    fn treats_an_empty_file_as_covered() {
        let empty = FileLines { file: "x.rs".into(), covered: 0, count: 0 };
        assert!((percent(&[&empty]) - 100.0).abs() < f64::EPSILON);
    }
}
