//! Stamps the build into slingerd for feedback notes (`src/feedback.rs`):
//! `SLINGER_BUILD_SHA` is the commit checked out and `SLINGER_BUILD_DIRTY`
//! is `1` when tracked files differ from it. A value already in the
//! environment wins, so a release build can name its own. Without git (a
//! source tarball) nothing is set and notes say `build=dev`.

use std::path::{Path, PathBuf};
use std::process::Command;

const SHA: &str = "SLINGER_BUILD_SHA";
const DIRTY: &str = "SLINGER_BUILD_DIRTY";

fn main() {
    for var in [SHA, DIRTY, "SLINGER_BUILD_SOURCE"] {
        println!("cargo:rerun-if-env-changed={var}");
    }
    if std::env::var_os(SHA).is_some() {
        return;
    }
    let Some(sha) = git(&["rev-parse", "HEAD"]) else { return };
    println!("cargo:rustc-env={SHA}={sha}");
    // No optional locks: a plain `git status` may rewrite the index, which
    // would make this script run again on every build.
    let status = ["--no-optional-locks", "status", "--porcelain", "--untracked-files=no"];
    let dirty = git(&status).is_some_and(|s| !s.is_empty());
    println!("cargo:rustc-env={DIRTY}={}", u8::from(dirty));

    // Run again when the commit moves or the sources change, so the stamp
    // follows the tree. A directory counts as changed when any file in it
    // does; a missing path would count as changed on every build, so only
    // paths that exist are named.
    let mut watch = vec!["HEAD".to_string(), "index".into(), "packed-refs".into()];
    watch.extend(git(&["symbolic-ref", "-q", "HEAD"]));
    let git_paths = watch.iter().filter_map(|f| git(&["rev-parse", "--git-path", f]));
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let sources = ["crates", "xtask", "Cargo.toml", "Cargo.lock"].map(|s| root.join(s));
    for path in git_paths.map(PathBuf::from).chain(sources) {
        if path.exists() {
            println!("cargo:rerun-if-changed={}", path.display());
        }
    }
}

/// What `git args` prints, trimmed, run in this crate; `None` when git is
/// missing or fails (not a checkout).
fn git(args: &[&str]) -> Option<String> {
    let out =
        Command::new("git").args(args).current_dir(env!("CARGO_MANIFEST_DIR")).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}
