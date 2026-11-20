//! check:agents: every repo path that AGENTS.md or ARCHITECTURE.md names in
//! backticks exists, so the agent instructions and the map never point at
//! something that is gone. Paths are from the repository root. Paths under
//! `target/` are build outputs: a fresh checkout has none, so they are not
//! checked for existence (they are still counted and listed by `referenced_paths`).

use std::path::Path;

use crate::files;

const DOCS: [&str; 2] = ["AGENTS.md", "ARCHITECTURE.md"];
const EXTENSIONS: &[&str] = &[".md", ".json", ".yml", ".yaml", ".toml", ".rs", ".sh", ".lock"];

pub fn check(root: &Path) -> Result<String, Vec<String>> {
    let mut errors = Vec::new();
    let mut said = Vec::new();
    for doc_name in DOCS {
        let doc = files::read(root, doc_name)?;
        let paths = referenced_paths(&doc);
        let built = paths.iter().filter(|p| is_build_output(p)).count();
        errors.extend(
            missing(&paths, |p| root.join(p).exists())
                .map(|p| format!("{doc_name}: path `{p}` does not exist")),
        );
        said.push(format!("{doc_name} {} paths ok", paths.len() - built));
        if built > 0 {
            said.push(format!("{built} build outputs under target/ not checked"));
        }
    }
    if errors.is_empty() { Ok(format!("check:agents: {}", said.join(", "))) } else { Err(errors) }
}

/// The source paths among `paths` that do not exist. A build output is never
/// missing: whether it exists depends on what was built, not on the repo.
fn missing<'a>(
    paths: &'a [String],
    exists: impl Fn(&str) -> bool + 'a,
) -> impl Iterator<Item = &'a String> + 'a {
    paths.iter().filter(move |p| !is_build_output(p) && !exists(p))
}

fn is_build_output(path: &str) -> bool {
    path == "target" || path.starts_with("target/")
}

/// Backticked tokens that look like repo paths: they contain `/` or end in a
/// known extension, and hold no space, URL, glob or placeholder.
pub fn referenced_paths(doc: &str) -> Vec<String> {
    doc.split('`')
        .skip(1)
        .step_by(2)
        .map(str::trim)
        .filter(|t| is_path_like(t))
        .map(|t| t.trim_end_matches('/').to_string())
        .collect()
}

fn is_path_like(t: &str) -> bool {
    let safe = |c: char| c.is_ascii_alphanumeric() || "._-/".contains(c);
    let first_ok = t.starts_with(|c: char| c.is_ascii_alphanumeric() || c == '.' || c == '_');
    let only_ext = t.starts_with('.') && !t[1..].contains(['.', '/']);
    first_ok
        && !only_ext
        && !t.contains("://")
        && !t.ends_with("...")
        && t.chars().all(safe)
        && (t.contains('/') || EXTENSIONS.iter().any(|e| t.ends_with(e)))
}

#[cfg(test)]
mod tests {
    use super::*;

    const NONE: [String; 0] = [];

    #[test]
    fn extracts_path_like_tokens() {
        let doc = "See `crates/core/` and `Cargo.toml`, run `cargo test`, not `.rs` or `<x>/y`.";
        assert_eq!(referenced_paths(doc), ["crates/core", "Cargo.toml"]);
    }

    #[test]
    fn skips_urls_globs_and_ellipses() {
        let doc = "`https://x.dev/a` `src/**/*.rs` `docs/...` `#[expect(lint)]`";
        assert_eq!(referenced_paths(doc), NONE);
    }

    #[test]
    fn a_build_output_is_not_missing_but_a_source_path_still_is() {
        let paths = ["target/slinger.app", "target", "targets/x.md", "docs/gone.md", "src/lib.rs"]
            .map(String::from);
        let gone: Vec<_> = missing(&paths, |p| p == "src/lib.rs").collect();
        assert_eq!(gone, ["targets/x.md", "docs/gone.md"]);
    }
}
