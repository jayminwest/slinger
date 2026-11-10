//! check:ci-parity: CI and the pre-commit hook run exactly `cargo xtask check-all`,
//! so local and CI gates match by construction. Every `run:` step in
//! `.github/workflows/ci*.yml` must be that one line; setup belongs in `uses:` steps.

use std::path::Path;

use crate::files;

const COMMAND: &str = "cargo xtask check-all";
const HOOK: &str = "scripts/hooks/pre-commit";

pub fn check(root: &Path) -> Result<String, Vec<String>> {
    let mut errors = Vec::new();
    let mut runs = 0;
    let dir = root.join(".github/workflows");
    let mut workflows: Vec<String> = std::fs::read_dir(&dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with("ci") && is_yaml(n))
        .collect();
    workflows.sort();
    for name in &workflows {
        let rel = format!(".github/workflows/{name}");
        let (n, bad) = run_steps(&files::read(root, &rel)?);
        runs += n;
        errors.extend(bad.into_iter().map(|(line, cmd)| {
            format!("{rel}:{line}: CI runs `{cmd}`; CI may only run `{COMMAND}` (put the check in a gate)")
        }));
    }
    if runs == 0 {
        errors.push(format!("no .github/workflows/ci*.yml step runs `{COMMAND}`"));
    }
    if !files::read(root, HOOK)?.contains(COMMAND) {
        errors.push(format!("{HOOK} does not run `{COMMAND}`"));
    }
    if errors.is_empty() {
        Ok(format!("check:ci-parity: {runs} CI step(s) run `{COMMAND}`"))
    } else {
        Err(errors)
    }
}

/// Counts the `run:` steps that are exactly the gate command, and returns the
/// others as `(line number, command)`.
pub fn run_steps(workflow: &str) -> (usize, Vec<(usize, String)>) {
    let mut ok = 0;
    let mut bad = Vec::new();
    for (i, line) in workflow.lines().enumerate() {
        let t = line.trim_start().trim_start_matches("- ").trim_start();
        let Some(cmd) = t.strip_prefix("run:") else { continue };
        let cmd = cmd.split(" #").next().unwrap_or_default().trim();
        if cmd == COMMAND || cmd == format!("{COMMAND} --bail") {
            ok += 1;
        } else {
            bad.push((i + 1, cmd.to_string()));
        }
    }
    (ok, bad)
}

fn is_yaml(name: &str) -> bool {
    Path::new(name)
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("yml") || e.eq_ignore_ascii_case("yaml"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_only_the_gate_command() {
        let wf = "steps:\n  - uses: actions/checkout@v6\n  - run: cargo xtask check-all\n";
        assert_eq!(run_steps(wf), (1, vec![]));
    }

    #[test]
    fn rejects_other_and_multiline_steps() {
        let wf = "  - run: cargo test\n  - run: |\n      cargo xtask check-all\n";
        let (ok, bad) = run_steps(wf);
        assert_eq!(ok, 0);
        assert_eq!(bad, vec![(1, "cargo test".to_string()), (2, "|".to_string())]);
    }
}
