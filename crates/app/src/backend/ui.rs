//! An agent UI's code, fetched from the daemon by reference (`name`,
//! `name@3` or a blob id): `ui_export` resolves it once and writes that
//! version's bytes to a file of the app's own, which is read and removed
//! (docs/service.md, "How a blob's bytes travel"). Blocks until the daemon
//! answers or its client gives up.

use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicU32, Ordering};

use slinger_control::Client;

use super::Problem;
use crate::ui::Code;

/// The code `reference` names.
///
/// # Errors
///
/// The daemon's, and a problem when its file cannot be read.
pub fn fetch(daemon: &Client, reference: &str) -> Result<Code, Problem> {
    static NEXT: AtomicU32 = AtomicU32::new(0);
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("slinger-app-ui-{}-{n}", std::process::id()));
    let read = (|| {
        let exported = daemon.ui_export(reference, &dir)?;
        let html = fs::read(&exported.path).map_err(|e| unreadable(&exported.path, &e))?;
        let ui = exported.ui;
        Ok(Code { name: ui.name, version: ui.version, hash: ui.hash, html })
    })();
    // The directory is the app's own and only this call writes it.
    let _ = fs::remove_dir_all(&dir);
    read
}

fn unreadable(path: &Path, e: &std::io::Error) -> Problem {
    let hint = Some("check the temporary directory is readable");
    Problem::new(format!("{}: {e}", path.display()), hint)
}

#[cfg(test)]
mod tests {
    use super::super::tests::with_daemon;
    use super::*;

    #[test]
    fn fetches_the_version_a_reference_names_by_its_hash() {
        with_daemon(|d| {
            let dir = tempfile::tempdir().unwrap();
            let file = dir.path().join("board.html");
            fs::write(&file, "<p>one</p>").unwrap();
            let first = d.ui_put("board", &file, "scout").unwrap().ui;
            fs::write(&file, "<p>two</p>").unwrap();
            d.ui_put("board", &file, "scout").unwrap();

            let latest = fetch(d, "board").unwrap();
            assert_eq!((latest.name.as_str(), latest.version), ("board", 2));
            assert_eq!(latest.html, b"<p>two</p>");
            let old = fetch(d, &first.hash).unwrap();
            assert_eq!((old.version, old.hash, old.html), (1, first.hash, b"<p>one</p>".to_vec()));
            assert_eq!(fetch(d, "board@1").unwrap().html, b"<p>one</p>");
        });
    }

    #[test]
    fn a_ui_that_is_not_there_is_a_problem_with_a_hint() {
        with_daemon(|d| {
            let problem = fetch(d, "nope").unwrap_err();
            assert!(problem.message.contains("nope"), "{problem:?}");
            assert!(problem.hint.is_some());
        });
    }
}
