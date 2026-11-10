//! check:binary-size: each release binary named in
//! `budgets/binary-size-budget.json` stays at or under its ceiling in bytes.
//! Ceilings only fall.

use std::path::{Path, PathBuf};

use crate::files;

const BUDGET: &str = "budgets/binary-size-budget.json";

pub fn check(root: &Path) -> Result<String, Vec<String>> {
    let config = files::read_json(root, BUDGET)?;
    let ceilings: Vec<(String, u64)> = config["max_bytes"]
        .as_object()
        .map(|m| m.iter().filter_map(|(k, v)| Some((k.clone(), v.as_u64()?))).collect())
        .unwrap_or_default();
    if ceilings.is_empty() {
        return Err(vec![format!("{BUDGET}: max_bytes must name at least one binary")]);
    }
    let mut build = vec!["build", "--release", "--locked"];
    for (bin, _) in &ceilings {
        build.extend(["--bin", bin]);
    }
    crate::run(root, "cargo", &build)?;
    let target = std::env::var_os("CARGO_TARGET_DIR").map_or(root.join("target"), PathBuf::from);
    let mut sizes = Vec::new();
    for (bin, max) in ceilings {
        let path = target.join("release").join(&bin);
        let size = std::fs::metadata(&path)
            .map_err(|e| vec![format!("cannot read {}: {e}", path.display())])?
            .len();
        sizes.push((bin, size, max));
    }
    let (report, errors) = evaluate(&sizes);
    if errors.is_empty() { Ok(report.join("\n")) } else { Err(errors) }
}

/// Checks each `(binary, size, ceiling)`. Returns the report lines and the errors.
pub fn evaluate(sizes: &[(String, u64, u64)]) -> (Vec<String>, Vec<String>) {
    let mut report = Vec::new();
    let mut errors = Vec::new();
    for (bin, size, max) in sizes {
        if size > max {
            errors.push(format!(
                "release binary {bin} is {size} bytes, over its {max}-byte ceiling in {BUDGET}"
            ));
        } else {
            report.push(format!("check:binary-size: {bin} is {size} of {max} bytes"));
        }
    }
    (report, errors)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passes_at_the_ceiling_and_fails_over_it() {
        let sizes = [("app".to_string(), 100, 100), ("big".to_string(), 101, 100)];
        let (report, errors) = evaluate(&sizes);
        assert_eq!(report, ["check:binary-size: app is 100 of 100 bytes"]);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("big is 101 bytes"));
    }
}
