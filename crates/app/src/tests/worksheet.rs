//! Acceptance for figures (plan pl-9f74c2, slinger-9dcf50, slinger-9ce9a6):
//! the worksheet (docs/design/figures.md, Appendix D) and the owner's
//! softmax worksheet (Appendix B, unchanged) drawn each by its one script
//! through `canvas_draw` show in the window as the core has them, the
//! user's pen writes over their answer areas and the packet says which, the
//! lasso takes only the ink, a `canvas_redraw` of the data fills the
//! agent's areas by name with every pen stroke kept, the same redraw again
//! writes nothing, and the agent's undo takes the figure away in one step.
//! docs/design/figures-acceptance.md has the runs by hand.

use std::path::Path;

use serde_json::{Value, json};

use super::figure::{bounds, said, selected};
use super::*;

const SCRIPTS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../script/src/tests");
const AGENT: &str = "agent:scout";

fn items(daemon: &control::Client) -> Vec<Value> {
    daemon.canvas_show("notes", None).unwrap().items
}

fn ink(daemon: &control::Client) -> Vec<Value> {
    items(daemon).into_iter().filter(|i| i["by"] == "user").collect()
}

/// The outline's line for item `id`.
fn line(out: &str, id: &Value) -> String {
    let id = id.as_str().unwrap();
    let head = format!("  {id} ");
    out.lines().find(|l| l.starts_with(&head)).unwrap_or_else(|| panic!("{id}: {out}")).into()
}

fn redraw(daemon: &control::Client, patch: &Path) -> Vec<u64> {
    let (canvas, by, name) = ("notes".into(), AGENT.into(), "worksheet".into());
    let ask = control::CanvasRedraw {
        canvas,
        by,
        name,
        data: Some(patch.into()),
        path: None,
        once: None,
    };
    daemon.canvas_redraw(ask).unwrap().seq
}

/// Ticks until the outline meets `met`.
fn shows(w: &mut World, met: impl Fn(&str) -> bool) {
    wait_until(|| {
        w.send(Event::Tick);
        met(&said(w))
    });
}

/// A small mark, about a letter's size, at `(x, y)` from the pen.
fn letter(w: &mut World, x: f64, y: f64, when: f64) {
    assert!(w.send(Event::Press(at(x, y, when))));
    w.send(Event::Drag(at(x + 5.0, y + 12.0, when + 0.05)));
    w.send(Event::Lift(at(x + 10.0, y, when + 0.1)));
}

/// The figure `name` from `<name>.js` (and `<name>.json` when `data`),
/// drawn by `agent:scout` at (20, 20): it has `count` items in 15 parts.
fn draw(daemon: &control::Client, name: &str, data: bool, count: usize) {
    let dir = Path::new(SCRIPTS);
    let drawn = daemon
        .canvas_draw(control::CanvasDraw {
            canvas: "notes".into(),
            by: AGENT.into(),
            name: name.into(),
            path: dir.join(format!("{name}.js")),
            data: data.then(|| dir.join(format!("{name}.json"))),
            place: control::Place::At([200, 200]),
            page: None,
            dry_run: false,
            png: None,
            once: None,
        })
        .unwrap();
    assert_eq!((drawn.items, drawn.parts.len()), (count, 15));
}

/// The window draws every item the core has, locked, with the core's
/// style: `styled` items dashed or dotted and `filled` filled.
fn drawn_as_the_core_has_it(w: &mut World, daemon: &control::Client, (styled, filled): (usize, usize)) {
    let sheet = items(daemon);
    bounds(w, sheet.len() as u64);
    let out = said(w);
    for item in &sheet {
        let l = line(&out, &item["id"]);
        assert!(l.ends_with(" locked"), "{l}");
        // A text's words could say "dashed": read what follows them.
        let style = l.rsplit('"').next().unwrap();
        assert_eq!(style.contains(" dashed"), item["dash"] == "dashed", "{l}");
        assert_eq!(style.contains(" dotted"), item["dash"] == "dotted", "{l}");
        assert_eq!(style.contains(" filled"), item.get("fill").is_some(), "{l}");
    }
    let has = |key: &str| sheet.iter().filter(|i| i.get(key).is_some()).count();
    assert_eq!((has("dash"), has("fill")), (styled, filled), "dashed and dotted areas, fills");
}

/// Three letters from the pen in each of the answer areas, the items `areas`.
fn answer(w: &mut World, areas: [u64; 2]) {
    let (a, b) = (bounds(w, areas[0]), bounds(w, areas[1]));
    for (when, area) in [(10.0, a), (20.0, b)] {
        for n in 0..3 {
            let x = area.x + 20.0 + 16.0 * f64::from(n);
            letter(w, x, area.y + 20.0, when + f64::from(n));
        }
    }
}

/// A lasso round the whole sheet, then escape; what it selected.
fn lasso_all(w: &mut World) -> Vec<u64> {
    w.key(Chord::new(CMD, '3'), "3");
    let crate::layout::Size { width, height } = w.shared.size;
    let (right, bottom) = (width - 100.0, height - 100.0);
    let around = [(1.0, 1.0), (right, 1.0), (right, bottom), (1.0, bottom)];
    assert!(w.send(Event::Press(at(1.0, 1.0, 40.0))));
    for (when, (x, y)) in
        [40.1, 40.2, 40.3, 40.4].into_iter().zip(around.into_iter().cycle().skip(1))
    {
        w.send(Event::Drag(at(x, y, when)));
    }
    w.send(Event::Lift(at(1.0, 1.0, 41.0)));
    let picked = selected(w);
    w.key(Chord::new(0, '\u{1b}'), "\u{1b}");
    picked
}

#[test]
fn the_worksheet_from_one_script_takes_the_pen_and_is_redrawn_by_name() {
    with_store(None, |daemon| {
        let _home = open_notes(daemon);
        let config = tempfile::tempdir().unwrap();
        let mut w = world(daemon, config.path());
        w.shared.size = crate::layout::Size { width: 1300.0, height: 1700.0 };
        draw(daemon, "worksheet", true, 56);
        drawn_as_the_core_has_it(&mut w, daemon, (6, 6));

        // The pen writes in two answer areas, over their locked boxes.
        answer(&mut w, [21, 23]);
        w.until(|_| ink(daemon).len() == 6);
        let ask = control::PacketAsk { canvas: "notes".into(), ..control::PacketAsk::default() };
        let packet = daemon.canvas_read(ask).unwrap().text;
        assert!(packet.contains("over=worksheet/1a-answer"), "{packet}");
        assert!(packet.contains("over=worksheet/1b-answer"), "{packet}");
        let ids: Vec<u64> =
            ink(daemon).iter().map(|i| i["id"].as_str().unwrap()[1..].parse().unwrap()).collect();
        assert_eq!(lasso_all(&mut w), ids, "the lasso takes the ink only");

        // The redraw fills the agent's areas by name and keeps every stroke.
        let written = ink(daemon);
        let patch = config.path().join("patch.json");
        let data = json!({"actual": ["C", "A", "E", "D", "B"], "feedback": "1a is right."});
        std::fs::write(&patch, data.to_string()).unwrap();
        assert_ne!(redraw(daemon, &patch), [] as [u64; 0]);
        assert_eq!(ink(daemon), written, "every pen stroke as it was");
        shows(&mut w, |out| {
            out.contains("\"1. C\" locked") && out.contains("\"1a is right.\" locked")
        });
        let next = daemon.canvas_show("notes", None).unwrap().next;
        assert_eq!(redraw(daemon, &patch), [] as [u64; 0], "the same redraw writes nothing");
        assert_eq!(daemon.canvas_show("notes", None).unwrap().next, next);

        // The agent's undo takes the redraw back, then the whole figure.
        let undo = || {
            let step = control::CanvasStep { canvas: "notes".into(), by: AGENT.into() };
            daemon.canvas_undo(step).unwrap();
        };
        undo();
        assert_eq!(items(daemon).len(), 56 + 6);
        undo();
        assert_eq!(items(daemon), written, "the ink is all that is left");
        shows(&mut w, |out| !out.contains(" locked"));
    });
}

#[test]
fn the_owners_softmax_worksheet_takes_the_pen_and_a_redraw_of_it_writes_nothing() {
    with_store(None, |daemon| {
        let _home = open_notes(daemon);
        let config = tempfile::tempdir().unwrap();
        let mut w = world(daemon, config.path());
        // Tall enough to show all of it at 100%: 1700 by 3802 units.
        w.shared.size = crate::layout::Size { width: 2000.0, height: 4100.0 };
        // Appendix B as the owner wrote it: 35 texts and 18 boxes, 9 dashed
        // and 6 dotted, 3 filled bars.
        draw(daemon, "softmax", false, 53);
        drawn_as_the_core_has_it(&mut w, daemon, (15, 3));

        // The pen writes in (a)'s and (b)'s answer boxes, items 5 and 9.
        answer(&mut w, [5, 9]);
        w.until(|_| ink(daemon).len() == 6);
        let ask = control::PacketAsk { canvas: "notes".into(), ..control::PacketAsk::default() };
        let packet = daemon.canvas_read(ask).unwrap().text;
        assert!(packet.contains("over=softmax/p1a "), "{packet}");
        assert!(packet.contains("over=softmax/p1b "), "{packet}");
        assert!(packet.contains(" figure softmax 20,20 1700x3802 parts=15 by=scout"), "{packet}");
        let ids: Vec<u64> =
            ink(daemon).iter().map(|i| i["id"].as_str().unwrap()[1..].parse().unwrap()).collect();
        assert_eq!(lasso_all(&mut w), ids, "the lasso takes the ink only");

        // The script reads no data: a redraw with a patch draws the same
        // items and writes nothing.
        let patch = config.path().join("patch.json");
        std::fs::write(&patch, json!({"note": 1}).to_string()).unwrap();
        let next = daemon.canvas_show("notes", None).unwrap().next;
        let ask = control::CanvasRedraw {
            canvas: "notes".into(),
            by: AGENT.into(),
            name: "softmax".into(),
            data: Some(patch),
            path: None,
            once: None,
        };
        let again = daemon.canvas_redraw(ask).unwrap();
        assert_eq!((again.seq, again.items), (Vec::new(), 53));
        assert_eq!(daemon.canvas_show("notes", None).unwrap().next, next);
    });
}
