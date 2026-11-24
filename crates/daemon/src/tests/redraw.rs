//! `canvas_redraw` and `canvas_figure` through the real socket: a redraw
//! patches the figure's last data and replaces it in place, the same data
//! again writes nothing, the user's ink stays, it follows the user's move,
//! another agent is refused by the author rule, and `canvas_figure` lists
//! the figures and gives one's script and data.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use slinger_control::{CanvasDraw, CanvasOps, CanvasRedraw, Client, EventFilter, Place};
use tempfile::TempDir;

use super::Daemon;

const SCRIPTS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../script/src/tests");

fn draw(client: &Client, script: &Path, data: Option<PathBuf>, place: Place) {
    let ask = CanvasDraw {
        canvas: "main".into(),
        by: "agent:scout".into(),
        name: "worksheet".into(),
        path: script.into(),
        data,
        place,
        page: None,
        dry_run: false,
        png: None,
        once: None,
    };
    client.canvas_draw(ask).unwrap();
}

fn worksheet(client: &Client) {
    let dir = Path::new(SCRIPTS);
    draw(
        client,
        &dir.join("worksheet.js"),
        Some(dir.join("worksheet.json")),
        Place::At([400, 400]),
    );
}

fn redraw(by: &str, data: Option<PathBuf>) -> CanvasRedraw {
    let (canvas, name) = ("main".into(), "worksheet".into());
    CanvasRedraw { canvas, by: by.into(), name, data, path: None, once: None }
}

/// A file in `dir` holding `text`.
fn file(dir: &TempDir, name: &str, text: &str) -> PathBuf {
    let path = dir.path().join(name);
    std::fs::write(&path, text).unwrap();
    path
}

fn drawn_events(client: &Client) -> Vec<Value> {
    let filter = EventFilter { kind: Some("figure_drawn".into()), ..EventFilter::default() };
    client.record_list(filter).unwrap()
}

fn items(client: &Client) -> Vec<Value> {
    client.canvas_show("main", None).unwrap().items
}

fn apply(client: &Client, by: &str, ops: Value) {
    let ask = CanvasOps { canvas: "main".into(), by: by.into(), ops, once: None };
    client.canvas_apply(ask).unwrap();
}

#[test]
fn a_redraw_patches_the_data_in_place_and_the_same_again_writes_nothing() {
    let daemon = Daemon::start();
    let client = daemon.client();
    client.canvas_create("main").unwrap();
    worksheet(&client);
    let ink =
        json!({ "page": "p1", "kind": "stroke", "width": 20, "pts": [1000, 1000, 1500, 1200] });
    apply(&client, "user", json!({ "op": "add", "item": ink }));
    let before = items(&client);
    let stroke = before.iter().find(|i| i["kind"] == "stroke").unwrap().clone();
    let next = client.canvas_show("main", None).unwrap().next;

    let dir = TempDir::new().unwrap();
    let same = client.canvas_redraw(redraw("agent:scout", None)).unwrap();
    assert_eq!((same.seq.len(), same.items, same.origin), (0, 56, [400, 400]), "nothing ran");
    let empty = file(&dir, "empty.json", "{}");
    let same = client.canvas_redraw(redraw("agent:scout", Some(empty))).unwrap();
    assert!(same.seq.is_empty(), "an empty patch is the same data");
    assert_eq!(client.canvas_show("main", None).unwrap().next, next, "no op written");
    assert_eq!(drawn_events(&client).len(), 1, "no event either");

    let patch = r#"{"actual": ["C", "A", "E", "D", "B"], "feedback": "1a is right."}"#;
    let patch = file(&dir, "patch.json", patch);
    let filled = client.canvas_redraw(redraw("agent:scout", Some(patch.clone()))).unwrap();
    assert_eq!((filled.items, filled.origin), (62, [400, 400]));
    assert!(!filled.seq.is_empty() && filled.seq.len() < 20, "only what changed: {:?}", filled.seq);
    let after = items(&client);
    assert!(after.contains(&stroke), "the user's stroke is as it was");
    let kept = before.iter().filter(|i| i["fig"] == "worksheet" && after.contains(i)).count();
    assert!(kept > 40, "most of the figure is untouched: {kept}");
    let events = drawn_events(&client);
    assert_eq!(events.len(), 2);
    let data = &events[1]["payload"]["data"];
    assert_eq!((&data["actual"][0], &data["feedback"]), (&json!("C"), &json!("1a is right.")));
    assert_eq!(data["rate"], 0.08, "the rest of the data is kept");
    assert_eq!(events[1]["payload"]["script"], events[0]["payload"]["script"]);

    let again = client.canvas_redraw(redraw("agent:scout", Some(patch))).unwrap();
    assert_eq!((again.seq.len(), again.items, drawn_events(&client).len()), (0, 62, 2));
    let unset = file(&dir, "unset.json", r#"{"actual": null, "feedback": null}"#);
    let back = client.canvas_redraw(redraw("agent:scout", Some(unset))).unwrap();
    assert_eq!(back.items, 56, "null takes the keys away");
    daemon.stop();
}

#[test]
fn a_redraw_follows_the_users_move_and_another_agent_is_refused() {
    let daemon = Daemon::start();
    let client = daemon.client();
    client.canvas_create("main").unwrap();
    worksheet(&client);
    let ids: Vec<Value> = items(&client).iter().map(|i| i["id"].clone()).collect();
    apply(&client, "user", json!({ "op": "move", "ids": ids, "dx": 500, "dy": 300 }));
    let dir = TempDir::new().unwrap();
    let patch = file(&dir, "p.json", r#"{"feedback": "Good."}"#);
    let moved = client.canvas_redraw(redraw("agent:scout", Some(patch.clone()))).unwrap();
    assert_eq!(moved.origin, [900, 700], "drawn again where the user put it");
    assert_eq!(drawn_events(&client)[1]["payload"]["origin"], json!([900, 700]));

    let e = client.canvas_redraw(redraw("agent:ada", Some(patch.clone()))).unwrap_err();
    assert_eq!(e.code(), "invalid", "{e}");
    assert!(e.to_string().contains("may not change figure worksheet"), "{e}");
    let mut gone = redraw("agent:scout", Some(patch));
    gone.name = "nothing".into();
    assert_eq!(client.canvas_redraw(gone).unwrap_err().code(), "not_found");
    daemon.stop();
}

#[test]
fn a_redraw_swaps_the_script_and_sees_the_item_it_was_drawn_over() {
    let daemon = Daemon::start();
    let client = daemon.client();
    client.canvas_create("main").unwrap();
    let item = json!({ "page": "p1", "kind": "box", "x": 1000, "y": 2000, "w": 3000, "h": 1500 });
    apply(&client, "user", json!({ "op": "add", "item": item }));
    let dir = TempDir::new().unwrap();
    let one = file(&dir, "one.js", "fig.box(0, 0, fig.view.target.w, 10);\n");
    draw(&client, &one, None, Place::Over("b1".into()));
    let two = file(&dir, "two.js", "fig.box(0, 0, fig.view.target.w / 2, 10);\nwarn('two');\n");
    let swap = CanvasRedraw { path: Some(two.clone()), ..redraw("agent:scout", None) };
    let drawn = client.canvas_redraw(swap).unwrap();
    assert_eq!((drawn.frame[2], drawn.seq.len()), (1500, 1), "half the box, set in place");
    assert_eq!(drawn.warnings, ["script:2:1: two"]);

    let one_figure = client.canvas_figure("main", Some("worksheet")).unwrap();
    let shown = &one_figure.figures[0];
    assert_eq!(
        (shown.by.as_str(), shown.items, shown.data.as_ref()),
        ("agent:scout", 1, None),
        "drawn with no data: the script read null"
    );
    let back = dir.path().join("back.js");
    client.blob_get(shown.script.as_deref().unwrap(), &back).unwrap();
    assert_eq!(std::fs::read(back).unwrap(), std::fs::read(two).unwrap(), "the script swapped in");
    daemon.stop();
}

#[test]
fn canvas_figure_lists_the_figures_and_their_parts() {
    let daemon = Daemon::start();
    let client = daemon.client();
    client.canvas_create("main").unwrap();
    assert_eq!(client.canvas_figure("main", None).unwrap().figures, []);
    worksheet(&client);
    let fig = |name: &str, x: i64| json!({ "kind": "box", "page": "p1", "x": x, "y": 0, "w": 10, "h": 10, "fig": name });
    apply(&client, "user", json!([{ "op": "add", "item": fig("hand", 0) }]));
    let all = client.canvas_figure("main", None).unwrap().figures;
    let names: Vec<&str> = all.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(names, ["hand", "worksheet"], "in reading order");
    let sheet = &all[1];
    assert_eq!((sheet.page.as_str(), sheet.items, sheet.parts.len()), ("p1", 56, 15));
    assert_eq!((sheet.locked, sheet.script.as_ref(), sheet.data.as_ref()), (true, None, None));
    let hand = client.canvas_figure("main", Some("hand")).unwrap().figures;
    assert_eq!((hand[0].by.as_str(), hand[0].script.as_ref()), ("user", None), "no script drew it");
    let e = client.canvas_redraw(CanvasRedraw { name: "hand".into(), ..redraw("user", None) });
    assert_eq!(e.unwrap_err().code(), "not_found", "only a figure a script drew is redrawn");
    let e = client.canvas_figure("main", Some("nothing")).unwrap_err();
    assert_eq!(
        (e.code(), e.hint().is_some_and(|h| h.contains("canvas figure main"))),
        ("not_found", true)
    );
    daemon.stop();
}
