use std::fs;
use std::path::Path;

use super::*;
use crate::backend::testing::wait_until;
use crate::backend::tests::{open_notes, with_daemon};
use crate::config::Config;
use crate::layout::WINDOW;
use crate::panes::{Kind, Pane};

#[expect(clippy::unnecessary_wraps, reason = "it stands in for platform::pdf::pages")]
fn two_pages(_: &Path) -> Result<Vec<(f64, f64)>, String> {
    Ok(vec![(612.0, 792.0), (612.0, 792.0)])
}

fn unreadable(_: &Path) -> Result<Vec<(f64, f64)>, String> {
    Err("paper.pdf is not a PDF this Mac can read".into())
}

/// Each `pdf` mark the panes draw: its page and its file.
fn pages(canvases: &mut Canvases, panes: &Panes) -> Vec<(u32, Option<PathBuf>)> {
    canvases.poll();
    let drawn = canvases.show(panes, WINDOW.rect(), &Config::default().theme);
    let marks = drawn.iter().flat_map(|p| p.scene.draws.clone());
    let pdfs = marks.filter_map(|d| match d.mark {
        Mark::Pdf { page, file, .. } => Some((page, file)),
        _ => None,
    });
    pdfs.collect()
}

#[test]
fn an_opened_pdf_shows_grey_then_its_pages_once_its_file_has_come() {
    with_daemon(|daemon| {
        let _notes = open_notes(daemon);
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("paper.pdf");
        fs::write(&file, b"%PDF-1.7 two pages").unwrap();
        let notes = Panes::new(Pane { kind: Kind::Canvas, name: "notes".into() });
        let mut canvases = Canvases::new(daemon.clone());
        // A cache of its own: the app's is shared by every test process and
        // worktree, and a fetched file is named by its bytes (slinger-f19088).
        let cache = tempfile::tempdir().unwrap();
        canvases.pdfs = Pdfs::start(daemon.clone(), cache.path().to_owned());
        canvases.open_pdf(&notes, Some("notes"), file.clone(), two_pages);
        wait_until(|| canvases.poll() && canvases.pdfs.opened.is_none());
        assert_eq!(canvases.problems().next(), None);
        assert!(daemon.canvas_list().unwrap().contains(&"paper.pdf".to_string()));

        let paper = Panes::new(Pane { kind: Kind::Canvas, name: "paper.pdf".into() });
        let mut shown = Vec::new();
        wait_until(|| {
            shown = pages(&mut canvases, &paper);
            !shown.is_empty() && shown.iter().all(|(_, f)| f.is_some())
        });
        // Page 2 lies under the pane's bottom edge: only page 1 is drawn.
        assert_eq!(shown.iter().map(|(p, _)| *p).collect::<Vec<_>>(), [1]);
        let fetched = shown[0].1.clone().unwrap();
        assert!(fetched.starts_with(cache.path()));
        assert_eq!(fs::read(&fetched).unwrap(), b"%PDF-1.7 two pages");
    });
}

#[test]
fn a_pdf_that_cannot_be_opened_is_a_problem_until_the_next() {
    with_daemon(|daemon| {
        let notes = Panes::new(Pane { kind: Kind::Canvas, name: "notes".into() });
        let mut canvases = Canvases::new(daemon.clone());
        let file = PathBuf::from("/nonexistent/paper.pdf");
        canvases.open_pdf(&notes, None, file.clone(), unreadable);
        wait_until(|| canvases.poll());
        let problem = canvases.problems().next().cloned().unwrap();
        assert_eq!(problem.message, "paper.pdf is not a PDF this Mac can read");
        canvases.open_pdf(&notes, None, file, two_pages);
        assert_eq!(canvases.problems().next(), None, "a new open clears it");
    });
}

#[test]
fn a_pdf_that_did_not_come_is_asked_for_again_the_next_time_it_is_drawn() {
    with_daemon(|daemon| {
        let _notes = open_notes(daemon);
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("paper.pdf");
        fs::write(&file, b"%PDF-1.7 one page").unwrap();
        let hash = daemon.blob_put(&file).unwrap();
        let pdf = serde_json::json!({ "op": "add", "item": { "page": "p1", "kind": "pdf",
            "x": 0, "y": 0, "w": 16000, "h": 20706, "hash": hash, "pdfpage": 1,
            "pagesize": [6120, 7920] } });
        let (canvas, by) = ("notes".to_string(), "user".to_string());
        let ops = slinger_control::CanvasOps { canvas, by, ops: pdf, once: None };
        daemon.canvas_apply(ops).unwrap();
        // The cache cannot be made while a file stands where its parent goes.
        let blocker = dir.path().join("blocked");
        fs::write(&blocker, "").unwrap();
        let mut canvases = Canvases::new(daemon.clone());
        canvases.pdfs = Pdfs::start(daemon.clone(), blocker.join("cache"));
        let notes = Panes::new(Pane { kind: Kind::Canvas, name: "notes".into() });
        wait_until(|| {
            pages(&mut canvases, &notes);
            matches!(canvases.pdfs.files.get(&hash), Some(Fetch::Missed(1, _)))
        });
        assert_eq!(pages(&mut canvases, &notes), [(1, None)], "grey while it waits");
        fs::remove_file(&blocker).unwrap();
        let mut shown = Vec::new();
        wait_until(|| {
            shown = pages(&mut canvases, &notes);
            shown.iter().all(|(_, f)| f.is_some())
        });
        assert_eq!(fs::read(shown[0].1.as_ref().unwrap()).unwrap(), b"%PDF-1.7 one page");
    });
}

#[test]
fn the_wait_before_asking_again_doubles_up_to_a_minute_or_so() {
    let waits: Vec<u64> = [1, 2, 3, 7, 8, 100].map(|n| wait(n).as_secs()).into();
    assert_eq!(waits, [1, 2, 4, 64, 64, 64]);
}
