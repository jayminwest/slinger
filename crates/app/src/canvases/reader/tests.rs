use std::fs;
use std::path::Path;

use crate::backend::testing::wait_until;
use crate::backend::tests::{open_notes, with_daemon};
use crate::canvases::Canvases;
use crate::config::Config;
use crate::layout::{Point, Viewport, WINDOW};
use crate::panes::{Kind, Pane, Panes};

#[expect(clippy::unnecessary_wraps, reason = "it stands in for platform::pdf::pages")]
fn three_pages(_: &Path) -> Result<Vec<(f64, f64)>, String> {
    Ok(vec![(612.0, 792.0); 3])
}

fn viewport(canvases: &mut Canvases, panes: &Panes) -> Viewport {
    canvases.poll();
    canvases.show(panes, WINDOW.rect(), &Config::default().theme);
    canvases.view(panes.focus()).viewport
}

#[test]
fn a_pdfs_canvas_fits_its_pages_to_the_width_and_scrolls_up_and_down_only() {
    with_daemon(|daemon| {
        let _notes = open_notes(daemon);
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("paper.pdf");
        fs::write(&file, b"%PDF-1.7 three pages").unwrap();
        let notes = Panes::new(Pane { kind: Kind::Canvas, name: "notes".into() });
        let mut canvases = Canvases::new(daemon.clone());
        canvases.open_pdf(&notes, Some("notes"), file, three_pages);
        wait_until(|| {
            canvases.poll() | daemon.canvas_list().unwrap().contains(&"paper.pdf".into())
        });

        let paper = Panes::new(Pane { kind: Kind::Canvas, name: "paper.pdf".into() });
        let pane = paper.focus();
        let mut at = viewport(&mut canvases, &paper);
        wait_until(|| {
            at = viewport(&mut canvases, &paper);
            at != Viewport::start()
        });
        let (column, size) = canvases.view(pane).reader.unwrap();
        assert_eq!((column.x, column.width), (0.0, 1600.0));
        assert_eq!(at, Viewport::start().reading(column, size), "fitted, at the top");

        canvases.pan(pane, 300.0, 0.0);
        assert_eq!(viewport(&mut canvases, &paper), at, "x is held");
        canvases.pan(pane, 0.0, 100.0);
        assert_eq!(viewport(&mut canvases, &paper), at, "the top stops it");
        canvases.zoom(pane, Point { x: 10.0, y: 10.0 }, 2.0);
        assert_eq!(viewport(&mut canvases, &paper), at, "a pinch zooms nothing");
        assert_eq!(canvases.said(), None, "and flashes nothing");
        canvases.pan(pane, 0.0, -1.0e6);
        let down = viewport(&mut canvases, &paper);
        let left = |v: Viewport| v.to_canvas(Point { x: 0.0, y: 0.0 }).x;
        assert_eq!(down.zoom(), at.zoom());
        assert!((left(down) - left(at)).abs() < 1e-9, "x is held");
        let bottom = down.to_screen(Point { x: 0.0, y: column.y + column.height }).y;
        assert!((bottom - (size.height - 16.0)).abs() < 1e-6, "the bottom stops it: {bottom}");

        // Any other canvas pans and zooms freely.
        let free = Panes::new(Pane { kind: Kind::Canvas, name: "notes".into() });
        let before = viewport(&mut canvases, &free);
        canvases.pan(free.focus(), 300.0, 0.0);
        assert_eq!(viewport(&mut canvases, &free), before.pan(300.0, 0.0));
    });
}
