//! `canvas_draw` through the real socket: a script's shapes land as one
//! figure, with `figure_drawn` holding the script and the data; the same
//! draw again writes nothing; a dry run writes nothing at all and can draw a
//! PNG; a failing script names its line; another maker's name is
//! `conflict`; a draw answers the agent's sling; a runaway script stops at
//! its limit and the daemon draws on.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use slinger_control::{CanvasDraw, CanvasOps, Client, EventFilter, NewEvent, NewTags, Place};
use tempfile::TempDir;

use super::Daemon;

const SCRIPTS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../script/src/tests");

fn worksheet() -> PathBuf {
    Path::new(SCRIPTS).join("worksheet.js")
}

fn ask(by: &str, name: &str, path: &Path, place: Place) -> CanvasDraw {
    CanvasDraw {
        canvas: "main".into(),
        by: by.into(),
        name: name.into(),
        path: path.into(),
        data: Some(Path::new(SCRIPTS).join("worksheet.json")),
        place,
        page: None,
        dry_run: false,
        png: None,
        once: None,
    }
}

/// A script in `dir`.
fn script(dir: &TempDir, text: &str) -> PathBuf {
    let path = dir.path().join("s.js");
    std::fs::write(&path, text).unwrap();
    path
}

fn events(client: &Client, kind: &str) -> Vec<Value> {
    client.record_list(EventFilter { kind: Some(kind.into()), ..EventFilter::default() }).unwrap()
}

fn next(client: &Client) -> u64 {
    client.canvas_show("main", None).unwrap().next
}

#[test]
fn a_script_draws_one_figure_and_the_same_draw_again_writes_nothing() {
    let daemon = Daemon::start();
    let client = daemon.client();
    client.canvas_create("main").unwrap();
    let drawn = client
        .canvas_draw(ask("agent:scout", "worksheet", &worksheet(), Place::At([400, 400])))
        .unwrap();
    assert_eq!((drawn.name.as_str(), drawn.page.as_str()), ("worksheet", "p1"));
    assert_eq!((drawn.items, drawn.parts.len(), drawn.origin), (56, 15, [400, 400]));
    assert_eq!((drawn.seq.len(), drawn.event), (56, None), "an op per item; nothing was slung");
    let items = client.canvas_show("main", None).unwrap().items;
    assert_eq!(items.len(), 56);
    assert!(items.iter().all(|i| i["fig"] == "worksheet" && i["by"] == "agent:scout"));
    let figures = events(&client, "figure_drawn");
    assert_eq!(figures.len(), 1);
    let payload = &figures[0]["payload"];
    let data: Value =
        serde_json::from_str(include_str!("../../../script/src/tests/worksheet.json")).unwrap();
    assert_eq!(
        (&payload["figure"], &payload["page"], &payload["data"]),
        (&json!("worksheet"), &json!("p1"), &data)
    );
    assert_eq!((&payload["origin"], &payload["frame"]), (&json!([400, 400]), &json!(drawn.frame)));
    assert_eq!(
        (&figures[0]["tags"]["canvas"], &figures[0]["tags"]["agent"]),
        (&json!("main"), &json!("scout"))
    );
    let dir = TempDir::new().unwrap();
    let back = dir.path().join("back.js");
    client.blob_get(payload["script"].as_str().unwrap(), &back).unwrap();
    assert_eq!(
        std::fs::read(&back).unwrap(),
        std::fs::read(worksheet()).unwrap(),
        "the script is the blob"
    );

    let again = client
        .canvas_draw(ask("agent:scout", "worksheet", &worksheet(), Place::At([400, 400])))
        .unwrap();
    assert_eq!((again.seq, again.frame, again.items), (vec![], drawn.frame, 56));
    assert_eq!(events(&client, "figure_drawn").len(), 1, "no event for a draw that wrote nothing");
    daemon.stop();
}

#[test]
fn a_dry_run_writes_nothing_and_draws_the_figure_alone_as_a_png() {
    let daemon = Daemon::start();
    let client = daemon.client();
    client.canvas_create("main").unwrap();
    let dir = TempDir::new().unwrap();
    let mut dry = ask("agent:scout", "worksheet", &worksheet(), Place::At([0, 0]));
    (dry.dry_run, dry.png) = (true, Some(dir.path().join("out").join("w.png")));
    let drawn = client.canvas_draw(dry).unwrap();
    assert_eq!((drawn.items, drawn.parts.len(), drawn.seq.len()), (56, 15, 0));
    let image = &drawn.images[0];
    assert_eq!(image.path, dir.path().join("out").join("w.png"));
    assert!(std::fs::read(&image.path).unwrap().starts_with(b"\x89PNG"), "a PNG");
    assert!(image.width > 100 && image.height > 100, "{image:?}");
    assert_eq!(next(&client), 1, "nothing written");
    assert_eq!(events(&client, "figure_drawn"), Vec::<Value>::new());
    daemon.stop();
}

#[test]
fn a_failing_script_names_its_line_and_writes_nothing() {
    let daemon = Daemon::start();
    let client = daemon.client();
    client.canvas_create("main").unwrap();
    let dir = TempDir::new().unwrap();
    let bad = script(&dir, "fig.box(0, 0, 10, 10);\nfig.box(0, 0, 'wide', 10);\n");
    let e = client.canvas_draw(ask("agent:scout", "f", &bad, Place::At([0, 0]))).unwrap_err();
    assert_eq!(e.code(), "invalid");
    assert!(e.to_string().contains("script:2:5: fig.box: w must be a number above 0"), "{e}");
    assert!(e.hint().is_some_and(|h| h.contains("--dry-run")), "{e:?}");
    let missing = dir.path().join("none.js");
    let e = client.canvas_draw(ask("agent:scout", "f", &missing, Place::At([0, 0]))).unwrap_err();
    assert_eq!(e.code(), "not_found");
    let mut no_json =
        ask("agent:scout", "f", &script(&dir, "fig.box(0, 0, 1, 1);"), Place::At([0, 0]));
    no_json.data = Some(script(&dir, "{nope"));
    assert_eq!(client.canvas_draw(no_json).unwrap_err().code(), "invalid");
    let e = client
        .canvas_draw(ask("agent:scout", "Not A Name", &worksheet(), Place::At([0, 0])))
        .unwrap_err();
    assert_eq!(e.code(), "invalid");
    assert_eq!((next(&client), events(&client, "figure_drawn").len()), (1, 0));
    daemon.stop();
}

/// Acceptance 4 of plan pl-9f74c2: a script that loops forever, eats
/// memory or recurses without end stops at its limit with a line, writes
/// nothing, and the same daemon then draws the worksheet.
#[test]
fn a_runaway_script_stops_at_its_limit_and_the_daemon_draws_on() {
    let daemon = Daemon::start();
    let client = daemon.client();
    client.canvas_create("main").unwrap();
    let dir = TempDir::new().unwrap();
    let runaway = [
        // The loop's line is not yet reliable (slinger-71c6cb): only "script:".
        ("const a = 1;\nwhile (true) {}\n", "script:", "ran longer than"),
        (
            "const xs = [];\nwhile (true) xs.push(new Array(100000).fill(1));\n",
            "script:2:",
            "used more than",
        ),
        ("function f(n) { return f(n + 1) + 1; }\nf(0);\n", "script:1:", "recursed too deep"),
    ];
    for (text, line, said) in runaway {
        let e = client.canvas_draw(ask("agent:scout", "r", &script(&dir, text), Place::At([0, 0])));
        let e = e.unwrap_err();
        assert_eq!(e.code(), "invalid");
        assert!(e.to_string().contains(line) && e.to_string().contains(said), "{e}");
    }
    assert_eq!(next(&client), 1, "nothing written");
    let drawn =
        client.canvas_draw(ask("agent:scout", "worksheet", &worksheet(), Place::At([0, 0])));
    assert_eq!(drawn.unwrap().items, 56);
    daemon.stop();
}

#[test]
fn a_script_sees_the_item_it_draws_over_and_the_page() {
    let daemon = Daemon::start();
    let client = daemon.client();
    client.canvas_create("main").unwrap();
    let item = json!({ "page": "p1", "kind": "box", "x": 1000, "y": 2000, "w": 3000, "h": 1500 });
    let ops = json!({ "op": "add", "item": item });
    client
        .canvas_apply(CanvasOps { canvas: "main".into(), by: "user".into(), ops, once: None })
        .unwrap();
    let dir = TempDir::new().unwrap();
    let seen = script(
        &dir,
        "warn(JSON.stringify([fig.view, data]));\nfig.box(0, 0, fig.view.target.w, 10);\n",
    );
    let mut over = ask("agent:scout", "mark", &seen, Place::Over("b1".into()));
    over.data = None;
    let drawn = client.canvas_draw(over).unwrap();
    assert_eq!(
        drawn.warnings,
        [r#"script:1:32: [{"target":{"x":0,"y":0,"w":300,"h":150},"page":"Page 1"},null]"#]
    );
    assert_eq!((drawn.origin, drawn.frame[2]), ([1000, 2000], 3000), "drawn over the box, as wide");
    daemon.stop();
}

#[test]
fn another_makers_figure_is_a_conflict_and_a_draw_answers_a_sling() {
    let daemon = Daemon::start();
    let client = daemon.client();
    client.canvas_create("main").unwrap();
    let tags =
        NewTags { agent: Some("scout".into()), canvas: Some("main".into()), ..NewTags::default() };
    let dir = TempDir::new().unwrap();
    let packet = client.blob_put(&script(&dir, "the packet")).unwrap();
    let payload = json!({ "prompt": "draw it", "packet": packet });
    let new = NewEvent { kind: "sling_sent".into(), source: "slinger".into(), tags, payload };
    let sling = client.record_append(new).unwrap()["number"].as_u64().unwrap();
    let two = script(&dir, "fig.box(0, 0, 10, 10);\nfig.box(20, 0, 10, 10);\n");
    let mut draw = ask("agent:scout", "pair", &two, Place::At([0, 0]));
    draw.data = None;
    let drawn = client.canvas_draw(draw.clone()).unwrap();
    let landed = events(&client, "reply_landed");
    assert_eq!(drawn.event, landed[0]["number"].as_u64());
    assert_eq!(landed[0]["payload"], json!({ "sling": sling, "items": ["i1", "i2"] }));
    draw.by = "agent:ada".into();
    let e = client.canvas_draw(draw).unwrap_err();
    assert_eq!((e.code(), e.to_string().contains("scout")), ("conflict", true), "{e}");
    daemon.stop();
}
