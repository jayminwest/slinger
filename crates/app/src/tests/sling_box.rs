//! Slinging a box (slinger-9923a6, docs/design/fold.md, "Sling a box")
//! through a real daemon: a folded box selected alone, or a pane stepped
//! into it, slings only what the box holds and names it (`in=b1`), and the
//! agent's answer put in the box shows in the pane inside it.

use std::time::Duration;

use super::step::{STEP_IN, apply, held, page, select_all, settle, stroke};
use super::*;

/// The prompt typed into the sling's entry, then return.
fn sling(w: &mut World, prompt: &str) {
    let area = w.sidebar.panes(w.shared.size.rect());
    // The sling before it closes once the daemon has it.
    wait_until(|| {
        w.send(Event::Tick);
        w.canvases.open_sling(&w.shared.panes, area)
    });
    w.canvases.sling_typed(prompt);
    assert!(w.canvases.sling_key(Chord::new(0, '\r')));
}

/// The text of the sling a listener takes: nobody runs the agent, so it
/// waits for one.
fn taken(w: &mut World, daemon: &control::Client) -> String {
    let mut text = String::new();
    wait_until(|| {
        w.send(Event::Tick);
        let heard = daemon.session_listen("notes", Duration::from_millis(100)).unwrap();
        text = heard.sling.map(|s| s.text).unwrap_or_default();
        !text.is_empty()
    });
    text
}

#[test]
fn a_box_alone_selected_slings_what_it_holds_and_so_does_a_pane_stepped_into_it() {
    with_store(None, |daemon| {
        let _home = open_notes(daemon);
        page(daemon);
        let ops = serde_json::json!({"op": "remove", "ids": ["i3"]});
        let (canvas, by) = ("notes".to_string(), "user".to_string());
        daemon.canvas_apply(control::CanvasOps { canvas, by, ops, once: None }).unwrap();
        let fold = control::CanvasFold {
            canvas: "notes".into(),
            item: "i1".into(),
            by: "user".into(),
            once: None,
        };
        daemon.canvas_fold(&control::FoldAsk::CanvasFold(fold)).unwrap();
        let config = tempfile::tempdir().unwrap();
        let mut w = world(daemon, config.path());
        select_all(&mut w, 1);
        // A stroke outside the box, after the selection: not in its sling.
        apply(daemon, &stroke(5500, 500));

        sling(&mut w, "what is this?");
        let text = taken(&mut w, daemon);
        let first = text.lines().next().unwrap_or_default();
        assert!(first.ends_with(" in=b1"), "the sling names the box: {text}");
        assert!(text.contains("c2 ink") && !text.contains("c4"), "only what it holds: {text}");
        assert!(text.contains("add `--in b1` to each `canvas put`"), "{text}");

        // Stepped into the box with nothing selected, a sling is of the box.
        select_all(&mut w, 2);
        assert!(w.send(Event::Key(STEP_IN)));
        assert!(w.canvases.selection().is_none());
        sling(&mut w, "and now?");
        let text = taken(&mut w, daemon);
        assert!(text.lines().next().unwrap_or_default().ends_with(" in=b1"), "{text}");
        assert!(text.contains("and now?"), "{text}");

        // The agent's answer in the box shows in the pane inside it.
        let put = control::CanvasPut {
            canvas: "notes".into(),
            agent: "scout".into(),
            beside: "c2".into(),
            what: control::Put::Text("a tick".into()),
            color: None,
            at: None,
            text_size: None,
            inside: Some("b1".into()),
        };
        let placed = daemon.canvas_put_as(put).unwrap();
        assert_eq!(held(daemon), serde_json::json!(["i2", placed.item]));
        let id: u64 = placed.item.trim_start_matches('i').parse().unwrap();
        settle(&mut w, |d| d.contains(&id) && d.contains(&2) && !d.contains(&4));
    });
}
