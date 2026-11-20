//! An agent's wrapped text keeps its rows on the daemon when it is resized
//! or edited (slinger-08633c), and a text typed here keeps its column
//! (slinger-d96bfb).

use super::*;
use crate::scene::canvas::Shape;
use crate::scene::cursor::Cursor;
use crate::scene::glyphs;

/// What the daemon holds of its one text: its text, size and rows.
fn stored(daemon: &Client) -> (String, f64, String) {
    let items = daemon.canvas_show("notes", None).unwrap().items;
    let item = crate::mirror::item(&items[0]).expect("a text");
    let Shape::Text { text, size, column, .. } = item.shape else { panic!("a text") };
    let rows = glyphs::rows(&text, size, column.unwrap());
    (text, size, rows)
}

/// The box the pane draws its one item in, in the window, once it does.
fn drawn(canvases: &mut Canvases, panes: &Panes) -> Option<Rect> {
    canvases.poll();
    let all = canvases.show(panes, WINDOW.rect(), &Config::default().theme);
    let pane = all.first()?;
    let d = pane.scene.draws.first()?;
    Some(Rect { x: d.bounds.x + pane.frame.x, y: d.bounds.y + pane.frame.y, ..d.bounds })
}

#[test]
fn a_wrapped_text_resized_then_edited_reads_back_from_the_daemon_in_its_rows() {
    with_store(None, |daemon| {
        let _home = open_notes(daemon);
        // Ten cells of 12: four rows.
        let text = "one two three four five six seven";
        let ops = serde_json::json!({"op": "add", "item": {"page": "p1", "kind": "text",
            "color": "#000000", "x": 1000, "y": 1000, "w": 1200, "text": text}});
        let (canvas, by) = ("notes".to_string(), "agent:scout".to_string());
        daemon.canvas_apply(slinger_control::CanvasOps { canvas, by, ops, once: None }).unwrap();
        let (mut canvases, panes) = (Canvases::new(daemon.clone()), notes());
        wait_until(|| drawn(&mut canvases, &panes).is_some());
        let (_, _, rows) = stored(daemon);
        assert_eq!(rows, "one two\nthree four\nfive six\nseven");

        let pane = (panes.focus(), WINDOW.rect());
        let b = drawn(&mut canvases, &panes).unwrap();
        let mid = (b.x + 10.0, b.y + b.height / 2.0);
        let tool = (Tool::Select, GREY);
        assert!(canvases.pen_down(&panes, pane, pen(mid.0, mid.1, 1.0), tool));
        canvases.pen_up(&panes, pen(mid.0, mid.1, 1.05), Some(pane));
        assert!(canvases.selection().is_some());
        // The bottom right handle, out from the corner.
        let corner = (b.x + b.width, b.y + b.height);
        let on = |c: &mut Canvases, x: f64, y: f64| {
            c.hover_handle(&panes, Some(pane), Point { x, y }, Tool::Select)
                || c.handle_cursor(pane.0).is_some()
        };
        let handle = (0..40)
            .map(|i| (corner.0 + f64::from(i) / 2.0, corner.1 + f64::from(i) / 2.0))
            .find(|&(x, y)| on(&mut canvases, x, y))
            .expect("a handle");
        assert!(canvases.pen_down(&panes, pane, pen(handle.0, handle.1, 2.0), tool));
        let to = (handle.0 + b.width * 0.37, handle.1 + b.height * 0.37);
        assert!(canvases.pen_drag(&panes, pen(to.0, to.1, 2.1), Some(pane)));
        canvases.pen_up(&panes, pen(to.0, to.1, 2.2), Some(pane));
        until(&mut canvases, &panes, daemon, |d| stored(d).1 > 24.0);
        let (_, size, after) = stored(daemon);
        assert_eq!(after, rows, "resized to {size}: the same rows after a reload");

        // A tap with the text tool edits it, in its column.
        let b = drawn(&mut canvases, &panes).unwrap();
        let mid = (b.x + 10.0, b.y + b.height / 2.0);
        assert!(canvases.pen_down(&panes, pane, pen(mid.0, mid.1, 4.0), (Tool::Text, GREY)));
        canvases.pen_up(&panes, pen(mid.0, mid.1, 4.1), Some(pane));
        assert!(canvases.editing());
        assert!(canvases.typed(" eight nine") && canvases.typed("\u{1b}"));
        until(&mut canvases, &panes, daemon, |d| stored(d).0.ends_with("nine"));
        let (_, _, edited) = stored(daemon);
        assert_eq!(edited, "one two\nthree four\nfive six\nseven\neight nine");
    });
}

#[test]
fn a_text_typed_here_keeps_its_column_when_edited_after_a_reload() {
    with_store(None, |daemon| {
        let _home = open_notes(daemon);
        let panes = notes();
        let mut canvases = Canvases::new(daemon.clone());
        wait_until(|| shown(&mut canvases, &panes) == 1);
        type_word(&mut canvases, &panes, (300.0, 300.0), 1.0);
        assert!(canvases.typed("\u{1b}"));
        until(&mut canvases, &panes, daemon, |d| count(d, "text") == 1);
        // A reload: a new window reads the text from the daemon.
        let mut canvases = Canvases::new(daemon.clone());
        wait_until(|| drawn(&mut canvases, &panes).is_some());
        let b = drawn(&mut canvases, &panes).unwrap();
        let pane = (panes.focus(), WINDOW.rect());
        let mid = (b.x + 5.0, b.y + b.height / 2.0);
        assert!(canvases.pen_down(&panes, pane, pen(mid.0, mid.1, 5.0), (Tool::Text, GREY)));
        canvases.pen_up(&panes, pen(mid.0, mid.1, 5.1), Some(pane));
        let more = " and a much longer line that runs past its column";
        assert!(canvases.typed(more) && canvases.typed("\u{1b}"));
        until(&mut canvases, &panes, daemon, |d| stored(d).0.ends_with("column"));
        let (_, _, rows) = stored(daemon);
        let first = "word and a much longer line that runs past its";
        assert_eq!(rows, format!("{first}\ncolumn"), "48 cells a row");
    });
}

/// slinger-d96bfb: the pointer over the right edge of the text being typed
/// shows the resize cursor, with the text tool too, and a drag from there
/// sets the column it wraps in.
#[test]
fn a_drag_of_the_typed_text_s_edge_sets_its_column() {
    with_store(None, |daemon| {
        let _home = open_notes(daemon);
        let panes = notes();
        let mut canvases = Canvases::new(daemon.clone());
        wait_until(|| shown(&mut canvases, &panes) == 1);
        type_word(&mut canvases, &panes, (300.0, 300.0), 1.0);
        assert!(canvases.typed(" two three"));
        let pane = (panes.focus(), WINDOW.rect());
        let y = 300.0;
        let edge = (300..1200).map(f64::from).find(|&x| {
            canvases.hover_handle(&panes, Some(pane), Point { x, y }, Tool::Text);
            canvases.handle_cursor(pane.0) == Some(Cursor::ResizeLeftRight)
        });
        let x = edge.expect("the edge of the text's column");
        assert!(x > 300.0 + 400.0, "out at its column, not its glyphs: {x}");
        assert!(canvases.pen_down(&panes, pane, pen(x, y, 2.0), (Tool::Text, GREY)));
        assert!(canvases.pen_drag(&panes, pen(x - 500.0, y, 2.1), Some(pane)));
        canvases.pen_up(&panes, pen(x - 500.0, y, 2.2), Some(pane));
        assert!(canvases.editing(), "the drag keeps the text typed");
        assert!(canvases.typed("\u{1b}"));
        until(&mut canvases, &panes, daemon, |d| count(d, "text") == 1);
        let (_, _, rows) = stored(daemon);
        assert_eq!(rows.lines().count(), 3, "{rows:?}");
    });
}
