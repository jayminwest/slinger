use std::fs;

use serde_json::json;

use super::super::tests::{open_notes, with_daemon};
use super::*;
use crate::scene::pdf::stack;

const LETTER: (f64, f64) = (612.0, 792.0);

/// How many pages [`letters`] reads: their `add`s are more than one
/// request line holds.
const PAGES: usize = 400;

/// [`PAGES`] Letter pages, as `platform` would read them.
#[expect(clippy::unnecessary_wraps, reason = "it stands in for platform::pdf::pages")]
fn letters(_: &Path) -> Result<Vec<(f64, f64)>, String> {
    Ok(vec![LETTER; PAGES])
}

fn locked(path: &Path) -> Result<Vec<(f64, f64)>, String> {
    Err(format!("{} is locked with a password", path.display()))
}

#[test]
fn a_blob_is_fetched_once_into_a_file_named_by_its_hash() {
    with_daemon(|d| {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("paper.pdf");
        fs::write(&source, b"%PDF-1.7 not really").unwrap();
        let hash = d.blob_put(&source).unwrap();
        let cache = dir.path().join("cache");
        let file = fetch(d, &hash, &cache).unwrap();
        assert_eq!(file, cache.join(format!("{}.pdf", &hash["sha256:".len()..])));
        assert_eq!(fs::read(&file).unwrap(), b"%PDF-1.7 not really");
        let gone = Client::new(dir.path().join("no-daemon.sock"));
        assert_eq!(fetch(&gone, &hash, &cache).unwrap(), file, "there already: not asked again");
        assert_eq!(fs::read_dir(&cache).unwrap().count(), 1, "no part file left");

        let missing = format!("sha256:{}", "0".repeat(64));
        let problem = fetch(d, &missing, &cache).unwrap_err();
        assert!(problem.message.contains("not"), "{problem:?}");
        let bad = fetch(d, "sha256:../../etc", &cache).unwrap_err();
        assert!(bad.message.contains("is not a blob id"), "{bad:?}");
    });
}

#[test]
fn fetches_of_one_blob_at_once_each_get_the_whole_file() {
    with_daemon(|d| {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("paper.pdf");
        let bytes: Vec<u8> = (0..1_000_000u32).map(|i| (i % 251) as u8).collect();
        fs::write(&source, &bytes).unwrap();
        let hash = d.blob_put(&source).unwrap();
        let cache = dir.path().join("cache");
        let files: Vec<_> = std::thread::scope(|s| {
            let fetches: Vec<_> = (0..8).map(|_| s.spawn(|| fetch(d, &hash, &cache))).collect();
            fetches.into_iter().map(|f| f.join().unwrap()).collect()
        });
        for file in files {
            let file = file.expect("every fetch gets the file");
            assert_eq!(fs::read(&file).unwrap(), bytes, "whole, not torn");
        }
        assert_eq!(fs::read_dir(&cache).unwrap().count(), 1, "no part file left");
    });
}

/// `item` is whole page `n` of the [`PAGES`] Letter pages of `hash`, on `page`.
fn is_page(item: &serde_json::Value, n: usize, hash: &str, page: &str) {
    let frame = stack(&[LETTER; PAGES])[n - 1];
    let tenths = |v: f64| (v * 10.0).round();
    assert_eq!(item["kind"], "pdf");
    assert_eq!(item["pdfpage"], n);
    assert_eq!(item["hash"], hash);
    assert_eq!(item["pagesize"], json!([6120, 7920]));
    assert_eq!(item["name"], "Paper (v2).pdf");
    assert_eq!(item["page"], page);
    assert_eq!(item["y"].as_f64(), Some(tenths(frame.y)));
    assert_eq!(item["w"].as_f64(), Some(tenths(frame.width)));
    assert_eq!(item.get("crop"), None);
}

#[test]
fn an_opened_pdf_is_a_canvas_of_its_pages_stacked_in_a_pane_beside_the_focused_one() {
    with_daemon(|d| {
        let _notes = open_notes(d);
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("Paper (v2).pdf");
        fs::write(&file, b"%PDF-1.7").unwrap();
        let ask = Opening { file: file.clone(), workspace: Some("notes".into()), beside: Some(1) };
        let scratch = dir.path().join("cache");
        assert_eq!(open(d, letters, &ask, &scratch).unwrap(), "paper--v2-.pdf");
        assert_eq!(fs::read_dir(&scratch).unwrap().count(), 0, "no operations file left");

        let shown = d.canvas_show("paper--v2-.pdf", None).unwrap();
        assert_eq!(shown.items.len(), PAGES, "every page, on one page");
        assert_eq!(shown.pages.len(), 1);
        let hash = d.blob_put(&file).unwrap();
        for n in [1, PAGES] {
            is_page(&shown.items[n - 1], n, &hash, &shown.page);
        }
        let layout = d.pane_list("notes").unwrap().layout;
        let panes = layout.panes();
        assert!(panes.iter().any(|p| p.target == "paper--v2-.pdf"), "{panes:?}");

        assert_eq!(open(d, letters, &ask, &scratch).unwrap(), "paper--v2--2.pdf", "id taken");
        let alone = Opening { workspace: None, beside: None, ..ask };
        assert_eq!(open(d, letters, &alone, &scratch).unwrap(), "paper--v2--3.pdf");
    });
}

#[test]
fn a_pdf_that_cannot_be_read_makes_nothing() {
    with_daemon(|d| {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("secret.pdf");
        fs::write(&file, b"%PDF-1.7").unwrap();
        let ask = Opening { file, workspace: None, beside: None };
        let scratch = dir.path().join("cache");
        let problem = open(d, locked, &ask, &scratch).unwrap_err();
        assert!(problem.message.ends_with("secret.pdf is locked with a password"), "{problem:?}");
        let none = |_: &Path| Ok(Vec::new());
        assert!(open(d, none, &ask, &scratch).unwrap_err().message.ends_with("has no pages"));
        assert!(!d.canvas_list().unwrap().iter().any(|c| c.starts_with("secret")));
    });
}

/// 40 pages, then one the canvas refuses: a size of 0.
#[expect(clippy::unnecessary_wraps, reason = "it stands in for platform::pdf::pages")]
fn torn(_: &Path) -> Result<Vec<(f64, f64)>, String> {
    let mut sizes = vec![LETTER; 40];
    sizes.push((0.0, 792.0));
    Ok(sizes)
}

#[test]
fn an_open_that_fails_partway_makes_no_canvas() {
    with_daemon(|d| {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("torn.pdf");
        fs::write(&file, b"%PDF-1.7").unwrap();
        let ask = Opening { file, workspace: None, beside: None };
        let scratch = dir.path().join("cache");
        let problem = open(d, torn, &ask, &scratch).unwrap_err();
        assert_eq!(problem.message, "the pdf item has no area", "the 41st page");
        let blocked = dir.path().join("blocked");
        fs::write(&blocked, b"a file, not a directory").unwrap();
        assert!(open(d, letters, &ask, &blocked).is_err(), "no operations file: no canvas");
        assert!(
            !d.canvas_list().unwrap().iter().any(|c| c.starts_with("torn")),
            "no partial canvas"
        );
        assert_eq!(fs::read_dir(&scratch).unwrap().count(), 0, "no operations file left");
    });
}
