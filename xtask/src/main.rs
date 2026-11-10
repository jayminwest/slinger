//! Quality gates. `cargo xtask check-all` runs every gate. It is the verify
//! command, and the pre-commit hook and CI run exactly it. `cargo xtask <gate>`
//! runs one gate with its full output; `cargo xtask --list` lists them.
//!
//! check-all output contract: one aligned `✓|✗ gate (N.Ns)` line per gate, a
//! one-line tally on success, and on failure only each failing gate's error
//! lines plus a re-run hint. `CHECK_ALL_VERBOSE=1` streams everything; `--bail`
//! stops at the first failure.
#![expect(clippy::print_stdout, clippy::print_stderr, reason = "xtask is a terminal tool")]

mod agents;
mod binsize;
mod coverage;
mod debt;
mod files;
mod layers;
mod parity;
mod size;

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};
use std::time::Instant;

/// A gate passes with a one-line summary or fails with error lines.
type Gate = fn(&Path) -> Result<String, Vec<String>>;

/// Ordered: cheap static gates first, then the gates that build (tests +
/// coverage, the release binary), the parity meta-gate last.
const GATES: &[(&str, Gate)] = &[
    ("lint", lint),
    ("check:agents", agents::check),
    ("check:layers", layers::check),
    ("check:deps", deps),
    ("check:size", size::check),
    ("check:debt", debt::check),
    ("check:coverage", coverage::check),
    ("check:binary-size", binsize::check),
    ("check:ci-parity", parity::check),
];

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let root = repo_root();
    match args.first().map(String::as_str) {
        Some("check-all") => check_all(&root, args.iter().any(|a| a == "--bail")),
        Some("--list") => {
            for (name, _) in GATES {
                println!("{name}");
            }
            ExitCode::SUCCESS
        }
        Some(name) => {
            if let Some((_, gate)) = GATES.iter().find(|(n, _)| *n == name) {
                run_one(&root, *gate)
            } else {
                eprintln!("✗ unknown gate '{name}'; run `cargo xtask --list`");
                ExitCode::from(2)
            }
        }
        None => {
            eprintln!(
                "usage: cargo xtask check-all [--bail] | cargo xtask <gate> | cargo xtask --list"
            );
            ExitCode::from(2)
        }
    }
}

fn repo_root() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest.parent().map_or(manifest.clone(), Path::to_path_buf)
}

fn run_one(root: &Path, gate: Gate) -> ExitCode {
    match gate(root) {
        Ok(summary) => {
            println!("{summary}");
            ExitCode::SUCCESS
        }
        Err(errors) => {
            for e in errors {
                eprintln!("error: {e}");
            }
            ExitCode::FAILURE
        }
    }
}

struct Outcome {
    gate: &'static str,
    ok: bool,
    output: String,
}

fn check_all(root: &Path, bail: bool) -> ExitCode {
    let verbose = std::env::var("CHECK_ALL_VERBOSE").is_ok_and(|v| v == "1");
    let width = GATES.iter().map(|(n, _)| n.len()).max().unwrap_or(0);
    let Ok(exe) = std::env::current_exe() else {
        eprintln!("✗ cannot locate the xtask binary");
        return ExitCode::FAILURE;
    };
    let start = Instant::now();
    let mut outcomes = Vec::new();
    for (gate, _) in GATES {
        if verbose {
            println!("\n── {gate} ──");
        }
        let t = Instant::now();
        let outcome = spawn_gate(&exe, root, gate, verbose);
        let mark = if outcome.ok { "✓" } else { "✗" };
        println!("{mark} {gate:width$} ({:.1}s)", t.elapsed().as_secs_f64());
        let failed = !outcome.ok;
        outcomes.push(outcome);
        if failed && bail {
            break;
        }
    }
    let failures: Vec<&Outcome> = outcomes.iter().filter(|o| !o.ok).collect();
    if failures.is_empty() {
        println!(
            "\n{}/{} gates passed ({:.1}s)",
            outcomes.len(),
            GATES.len(),
            start.elapsed().as_secs_f64()
        );
        return ExitCode::SUCCESS;
    }
    let names: Vec<&str> = failures.iter().map(|f| f.gate).collect();
    eprintln!("\n{} gate(s) failed: {}\n", failures.len(), names.join(", "));
    for f in failures {
        eprintln!("── {} ──", f.gate);
        for line in signatures(&f.output) {
            eprintln!("  {}", clip(line, 200));
        }
        eprintln!(
            "  ↳ re-run: cargo xtask {}  (or CHECK_ALL_VERBOSE=1 cargo xtask check-all)",
            f.gate
        );
    }
    ExitCode::FAILURE
}

fn spawn_gate(exe: &Path, root: &Path, gate: &'static str, verbose: bool) -> Outcome {
    let mut cmd = Command::new(exe);
    cmd.arg(gate).current_dir(root);
    if verbose {
        let ok = cmd.status().is_ok_and(|s| s.success());
        return Outcome { gate, ok, output: String::new() };
    }
    match cmd.stdin(Stdio::null()).output() {
        Ok(out) => Outcome {
            gate,
            ok: out.status.success(),
            output: format!(
                "{}\n{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            ),
        },
        Err(e) => Outcome { gate, ok: false, output: format!("error: cannot run gate: {e}") },
    }
}

/// The failure-relevant lines of a gate's output: compiler, test and gate
/// errors when there are any, else the last 25 non-empty lines.
fn signatures(output: &str) -> Vec<&str> {
    const MARKERS: &[&str] = &["error", "✗", "---- ", "test result: FAILED", "Diff in "];
    let lines: Vec<&str> =
        output.lines().map(str::trim_end).filter(|l| !l.trim().is_empty()).collect();
    let hits: Vec<&str> = lines
        .iter()
        .copied()
        .filter(|l| {
            let t = l.trim_start();
            MARKERS.iter().any(|m| t.starts_with(m)) || t.contains("panicked at")
        })
        .take(50)
        .collect();
    if hits.is_empty() { lines[lines.len().saturating_sub(25)..].to_vec() } else { hits }
}

/// `line` cut to `max` chars; rustc and cargo echo whole command lines on failure.
fn clip(line: &str, max: usize) -> String {
    match line.char_indices().nth(max) {
        Some((at, _)) => format!("{}…", &line[..at]),
        None => line.to_string(),
    }
}

/// Runs a command in `root` with inherited output; a non-zero exit is one error line.
fn run(root: &Path, program: &str, args: &[&str]) -> Result<(), Vec<String>> {
    let status = Command::new(program).args(args).current_dir(root).status();
    match status {
        Ok(s) if s.success() => Ok(()),
        Ok(s) => Err(vec![format!("`{program} {}` exited with {s}", args.join(" "))]),
        Err(e) => Err(vec![format!("cannot run `{program}`: {e}")]),
    }
}

fn lint(root: &Path) -> Result<String, Vec<String>> {
    run(root, "cargo", &["fmt", "--all", "--check"])?;
    run(
        root,
        "cargo",
        &["clippy", "--workspace", "--all-targets", "--locked", "--", "-D", "warnings"],
    )?;
    Ok("lint: fmt and clippy clean".into())
}

fn deps(root: &Path) -> Result<String, Vec<String>> {
    // Direct, not `cargo machete`: under `cargo run` it misreads its own argv.
    run(root, "cargo-machete", &[])?;
    run(root, "cargo", &["deny", "--locked", "check"])?;
    Ok("check:deps: no unused, vulnerable or disallowed dependencies".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signatures_prefer_error_lines() {
        let out =
            "Compiling x\nerror[E0425]: cannot find value\n  --> src/lib.rs:1:1\nwarning: y\n";
        assert_eq!(signatures(out), vec!["error[E0425]: cannot find value"]);
    }

    #[test]
    fn clip_cuts_long_lines_on_a_char_boundary() {
        assert_eq!(clip("ab✓d", 3), "ab✓…");
        assert_eq!(clip("abc", 3), "abc");
    }

    #[test]
    fn signatures_fall_back_to_the_tail() {
        let out = (0..40).map(|i| format!("line {i}")).collect::<Vec<_>>().join("\n");
        let sig = signatures(&out);
        assert_eq!(sig.len(), 25);
        assert_eq!(sig.last(), Some(&"line 39"));
    }

    #[test]
    fn gate_names_are_unique_and_parity_is_last() {
        let names: Vec<&str> = GATES.iter().map(|(n, _)| *n).collect();
        let mut sorted = names.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), names.len());
        assert_eq!(names.last(), Some(&"check:ci-parity"));
    }
}
