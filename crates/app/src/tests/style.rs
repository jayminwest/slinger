//! Styled items (slinger-450877, docs/design/figures.md): the window draws
//! a box's fill and dash, a closed stroke's dots and fill and an arrow's
//! heads, a driven window's outline says them, and a duplicate keeps them.

use serde_json::json;

use super::step::{apply, select_all};
use super::*;

/// What the driven window says now.
fn said(w: &mut World) -> String {
    let scene = view(&w.settings, &mut w.shared, &w.tools, &w.sidebar, &mut w.canvases);
    crate::drive::outline(&scene)
}

/// The line the outline says for item `id`.
fn line(said: &str, id: &str) -> String {
    let start = format!("  {id} ");
    said.lines().find(|l| l.starts_with(&start)).unwrap_or_default().to_string()
}

#[test]
fn a_styled_item_draws_says_and_keeps_its_fill_dash_and_heads() {
    with_store(None, |daemon| {
        let _home = open_notes(daemon);
        let config = tempfile::tempdir().unwrap();
        let mut w = world(daemon, config.path());
        apply(
            daemon,
            &json!({"page": "p1", "kind": "box", "color": "#000000", "x": 1000, "y": 1000,
                "w": 2000, "h": 1000, "fill": "#ff0000", "dash": "dashed"}),
        );
        apply(
            daemon,
            &json!({"page": "p1", "kind": "stroke", "color": "#000000", "width": 20,
                "pts": [4000, 1000, 1000, 0, 0, 1000, -1000, 0], "closed": true,
                "fill": "#00ff0033", "dash": "dotted"}),
        );
        apply(
            daemon,
            &json!({"page": "p1", "kind": "arrow", "color": "#000000",
                "pts": [1000, 3000, 2000, 0], "heads": "both", "dash": "dashed"}),
        );
        apply(
            daemon,
            &json!({"page": "p1", "kind": "arrow", "color": "#000000",
                "pts": [1000, 4000, 2000, 0], "heads": "none"}),
        );
        let mut out = String::new();
        wait_until(|| {
            w.send(Event::Tick);
            out = said(&mut w);
            out.contains("  i4 ")
        });
        // A fill is drawn at most 40%: #66 of #ff.
        assert!(line(&out, "i1").ends_with("box width 2 dashed filled #ff000066"), "{out}");
        assert!(line(&out, "i2").ends_with(" dotted filled #00ff0033"), "{out}");
        assert!(line(&out, "i3").ends_with(" heads both dashed"), "{out}");
        assert!(line(&out, "i4").ends_with(" heads none"), "{out}");
        // A duplicate is written with the style of what it copies.
        select_all(&mut w, 4);
        w.key(Chord::new(CMD, 'j'), "");
        let styled = |items: &[serde_json::Value], key: &str| {
            items.iter().filter(|i| i.get(key).is_some()).count()
        };
        wait_until(|| {
            w.send(Event::Tick);
            daemon.canvas_show("notes", None).unwrap().items.len() == 8
        });
        let items = daemon.canvas_show("notes", None).unwrap().items;
        assert_eq!(
            (styled(&items, "fill"), styled(&items, "dash"), styled(&items, "heads")),
            (4, 6, 4),
            "{items:?}"
        );
        assert_eq!(styled(&items, "closed"), 2);
    });
}
