//! Repo file access shared by the gates.

use std::path::Path;

/// Files under `dirs` (relative to `root`) with an extension in `exts`, as sorted
/// `/`-separated paths relative to `root`. Skips `target/` and dot directories.
pub fn under(root: &Path, dirs: &[&str], exts: &[&str]) -> Vec<String> {
    let mut out = Vec::new();
    for dir in dirs {
        walk(root, &root.join(dir), exts, &mut out);
    }
    out.sort();
    out
}

fn walk(root: &Path, dir: &Path, exts: &[&str], out: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if path.is_dir() {
            if name != "target" && !name.starts_with('.') {
                walk(root, &path, exts, out);
            }
        } else if path.extension().is_some_and(|e| exts.iter().any(|x| e == *x)) {
            out.push(relative(root, &path));
        }
    }
}

/// `path` relative to `root` with `/` separators; unchanged if outside `root`.
pub fn relative(root: &Path, path: &Path) -> String {
    let rel = path.strip_prefix(root).unwrap_or(path);
    rel.to_string_lossy().replace('\\', "/")
}

pub fn read(root: &Path, rel: &str) -> Result<String, Vec<String>> {
    std::fs::read_to_string(root.join(rel)).map_err(|e| vec![format!("cannot read {rel}: {e}")])
}

pub fn read_json(root: &Path, rel: &str) -> Result<serde_json::Value, Vec<String>> {
    serde_json::from_str(&read(root, rel)?)
        .map_err(|e| vec![format!("{rel} is not valid JSON: {e}")])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_matching_files_relative_and_sorted() {
        let root = std::env::temp_dir().join(format!("xtask-files-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for f in ["src/b.rs", "src/a.rs", "src/notes.md", "src/target/x.rs", "src/.hidden/y.rs"] {
            let p = root.join(f);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, "").unwrap();
        }
        assert_eq!(under(&root, &["src", "missing"], &["rs"]), ["src/a.rs", "src/b.rs"]);
        std::fs::remove_dir_all(&root).unwrap();
    }
}
