//! `slinger canvas redraw` and `slinger canvas figure` against a real
//! daemon: a redraw patches the data and is silent, the same again writes
//! nothing; `figure` lists the figures, one's parts, its script and data;
//! and each misuse gets its exit code.

use super::{cli_each, json};
use crate::output::exit;

const SCRIPT: &str = "fig.box(0, 0, (data || {}).w || 10, 10, { name: 'a' });\n";

#[test]
fn redraw_patches_the_data_and_figure_prints_what_was_drawn() {
    let dir = tempfile::tempdir().unwrap();
    let script = dir.path().join("f.js");
    std::fs::write(&script, SCRIPT).unwrap();
    let script = script.to_str().unwrap();
    let draw =
        ["canvas", "draw", "main", script, "--name", "f", "--at", "40,40", "--agent", "scout"];
    let runs = cli_each(&[
        &["canvas", "create", "main"],
        &[&draw[..], &["--data", r#"{"w": 30, "keep": 1}"#]].concat(),
        &["canvas", "redraw", "main", "f", "--data", r#"{"w": 50}"#, "--agent", "scout"],
        &["canvas", "redraw", "main", "f", "--data", r#"{"w": 50}"#, "--agent", "scout", "--json"],
        &["canvas", "figure", "main"],
        &["canvas", "figure", "main", "f"],
        &["canvas", "figure", "main", "f", "--data"],
        &["canvas", "figure", "main", "f", "--script"],
        &["canvas", "figure", "main", "--json"],
    ]);
    for r in &runs {
        assert_eq!((r.code, r.err.as_str()), (exit::OK, ""), "{}", r.out);
    }
    assert_eq!(runs[2].out, "", "silent");
    let same = json(&runs[3]);
    assert_eq!((&same["command"], &same["seq"]), (&"canvas redraw".into(), &serde_json::json!([])));
    assert_eq!(same["frame"], serde_json::json!([40, 40, 500, 100]));
    assert_eq!(runs[4].out, "f p1 4,4 50x10 by=scout parts=1\n");
    assert_eq!(runs[5].out, "a 4,4 50x10 items=1\n");
    assert_eq!(runs[6].out, "{\"keep\":1,\"w\":50}\n");
    assert_eq!(runs[7].out, SCRIPT);
    let all = json(&runs[8]);
    assert_eq!(
        (&all["figures"][0]["name"], &all["figures"][0]["by"]),
        (&"f".into(), &"agent:scout".into())
    );
}

#[test]
fn a_redraw_or_a_figure_misused_gets_its_exit_code() {
    let dir = tempfile::tempdir().unwrap();
    let script = dir.path().join("f.js");
    std::fs::write(&script, SCRIPT).unwrap();
    let script = script.to_str().unwrap();
    let runs = cli_each(&[
        &["canvas", "create", "main"],
        &["canvas", "draw", "main", script, "--name", "f", "--at", "0,0", "--agent", "scout"],
        &["canvas", "redraw", "main", "f", "--agent", "scout"],
        &["canvas", "redraw", "main", "f", "--data", "{}", "--agent", "ada"],
        &["canvas", "redraw", "main", "g", "--data", "{}", "--agent", "scout"],
        &["canvas", "redraw", "main", "f", "--data", "{nope", "--agent", "scout"],
        &["canvas", "figure", "main", "--script"],
        &["canvas", "figure", "main", "g"],
    ]);
    let codes: Vec<u8> = runs.iter().map(|r| r.code).collect();
    assert_eq!(
        codes,
        [
            exit::OK,
            exit::OK,
            exit::USAGE,
            exit::INVALID,
            exit::NOT_FOUND,
            exit::USAGE,
            exit::USAGE,
            exit::NOT_FOUND
        ]
    );
    assert!(runs[3].err.contains("may not change figure f"), "{}", runs[3].err);
    assert!(runs[7].err.contains("canvas figure main"), "{}", runs[7].err);
}
