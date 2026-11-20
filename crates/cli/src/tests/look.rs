//! `slinger canvas crop` and `slinger canvas strokes` against a real daemon.

use super::{cli_each, json};
use crate::output::exit;

const INK: &str = r#"[{"op":"add","item":{"page":"p1","kind":"stroke","width":20,"pts":[0,0,100,0]}},
{"op":"add","item":{"page":"p1","kind":"box","x":0,"y":500,"w":1000,"h":500}}]"#;

#[test]
fn crop_prints_the_images_it_wrote_and_strokes_the_points() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("shot.png");
    let file = file.to_str().unwrap();
    let runs = cli_each(&[
        &["canvas", "create", "main"],
        &["canvas", "apply", "main", INK],
        &["canvas", "crop", "main", "--item", "b2", "--out", file],
        &[
            "canvas",
            "crop",
            "main",
            "--box",
            "-100,0,100000,500",
            "--page",
            "Page 1",
            "--out",
            file,
        ],
        &["canvas", "crop", "main", "--item", "c1", "--out", file, "--json"],
        &["canvas", "strokes", "main", "--item", "c1"],
        &["canvas", "strokes", "main", "--item", "i1", "--json"],
    ]);
    for r in &runs {
        assert_eq!((r.code, r.err.as_str()), (exit::OK, ""), "{}", r.out);
    }
    assert_eq!(runs[2].out, format!("{file}\n"));
    let tiles: Vec<&str> = runs[3].out.lines().collect();
    assert_eq!(tiles.len(), 3);
    assert!(tiles[2].ends_with("shot-3.png"), "{}", tiles[2]);
    let crop = json(&runs[4]);
    assert_eq!(
        (&crop["command"], &crop["images"][0]["path"]),
        (&"canvas crop".into(), &file.into())
    );
    assert!(crop["images"][0]["width"].as_u64().unwrap() > 1);
    let stroke: serde_json::Value = serde_json::from_str(runs[5].out.trim()).unwrap();
    assert_eq!(stroke["points"], serde_json::json!([[0, 0], [100, 0]]));
    assert_eq!(json(&runs[6])["strokes"][0]["id"], "i1");
}

#[test]
fn crop_needs_one_of_item_and_box_and_unknown_ids_are_not_found() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("x.png");
    let file = file.to_str().unwrap();
    let runs = cli_each(&[
        &["canvas", "create", "main"],
        &["canvas", "crop", "main", "--out", file],
        &["canvas", "crop", "main", "--item", "i1", "--box", "0,0,1,1"],
        &["canvas", "crop", "main", "--item", "i1", "--page", "p1"],
        &["canvas", "crop", "main", "--item", "i9", "--out", file],
        &["canvas", "strokes", "main", "--item", "c9"],
        &["canvas", "crop", "main", "--box", "0,0,1", "--out", file],
    ]);
    let codes: Vec<u8> = runs.iter().map(|r| r.code).collect();
    let usage = exit::USAGE;
    assert_eq!(
        codes,
        [exit::OK, usage, usage, usage, exit::NOT_FOUND, exit::NOT_FOUND, exit::INVALID]
    );
    assert!(runs[4].err.contains("item i9 not found"), "{}", runs[4].err);
}

#[test]
fn find_prints_page_item_and_text_per_match_and_nothing_for_none() {
    let texts = r#"[{"op":"page.add","name":"Two"},
{"op":"add","item":{"page":"p2","kind":"text","x":0,"y":0,"text":"plan\nB"}},
{"op":"add","item":{"page":"p1","kind":"text","x":0,"y":0,"text":"The plan"}}]"#;
    let runs = cli_each(&[
        &["canvas", "create", "main"],
        &["canvas", "apply", "main", texts],
        &["canvas", "find", "main", "PLAN"],
        &["canvas", "find", "main", "nowhere"],
        &["canvas", "find", "main", "plan b", "--json"],
        &["canvas", "find", "nope", "plan"],
    ]);
    assert_eq!(runs[2].out, "p1\ti2\tThe plan\np2\ti1\tplan\\nB\n");
    assert_eq!((runs[3].code, runs[3].out.as_str()), (exit::OK, ""));
    assert_eq!(json(&runs[4])["found"][0]["id"], "i1");
    assert_eq!(runs[5].code, exit::NOT_FOUND);
}
