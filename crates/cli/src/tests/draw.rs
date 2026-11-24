//! `slinger canvas draw` against a real daemon: silent on success but for
//! the script's warnings, `--at` in canvas units, data inline or from a
//! file, a dry run that writes nothing but the PNG, and a failing script's
//! line in the error.

use super::{cli_each, json};
use crate::output::exit;

const SCRIPT: &str =
    "part('a', () => box(0, 0, fig.data.w || 10, 10));\nwarn('look at ' + fig.view.page);\n";

#[test]
fn draw_prints_only_the_warnings_and_places_the_figure_at_canvas_units() {
    let dir = tempfile::tempdir().unwrap();
    let script = dir.path().join("f.js");
    std::fs::write(&script, SCRIPT).unwrap();
    let data = dir.path().join("d.json");
    std::fs::write(&data, r#"{"w": 30}"#).unwrap();
    let (script, at_data) = (script.to_str().unwrap(), format!("@{}", data.display()));
    let runs = cli_each(&[
        &["canvas", "create", "main"],
        &[
            "canvas",
            "draw",
            "main",
            script,
            "--name",
            "f",
            "--at",
            "Page 1:4,4.5",
            "--agent",
            "scout",
        ],
        &["canvas", "show", "main"],
        &[
            "canvas",
            "draw",
            "main",
            script,
            "--name",
            "g",
            "--beside",
            "f",
            "--data",
            r#"{"w": 20}"#,
            "--json",
        ],
        &[
            "canvas", "draw", "main", script, "--name", "h", "--at", "0,0", "--data", &at_data,
            "--json",
        ],
    ]);
    for r in &runs {
        assert_eq!((r.code, r.err.as_str()), (exit::OK, ""), "{}", r.out);
    }
    assert_eq!(runs[1].out, "warning: script:2:19: look at Page 1\n");
    assert!(runs[2].out.contains(r#""w":100,"x":40,"y":45"#), "{}", runs[2].out);
    assert!(
        runs[2].out.contains(
            r#""fig":"f","h":100,"id":"i1","kind":"box","locked":true,"page":"p1","part":"a""#
        ),
        "{}",
        runs[2].out
    );
    let beside = json(&runs[3]);
    assert_eq!((&beside["command"], &beside["name"]), (&"canvas draw".into(), &"g".into()));
    assert_eq!(
        (&beside["frame"][2], &beside["parts"], &beside["items"]),
        (&200.into(), &serde_json::json!(["a"]), &1.into())
    );
    assert_eq!(json(&runs[4])["frame"][2], 300, "data from the file");
}

#[test]
fn a_dry_run_prints_its_png_and_writes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let script = dir.path().join("f.js");
    std::fs::write(&script, "box(0, 0, 100, 50, { fill: true, dash: 'dashed' });").unwrap();
    let png = dir.path().join("f.png");
    let (script, png_path) = (script.to_str().unwrap(), png.to_str().unwrap());
    let runs = cli_each(&[
        &["canvas", "create", "main"],
        &[
            "canvas",
            "draw",
            "main",
            script,
            "--name",
            "f",
            "--at",
            "0,0",
            "--dry-run",
            "--png",
            png_path,
        ],
        &["canvas", "show", "main"],
    ]);
    for r in &runs {
        assert_eq!((r.code, r.err.as_str()), (exit::OK, ""), "{}", r.out);
    }
    assert_eq!(runs[1].out, format!("{png_path}\n"));
    assert!(std::fs::read(&png).unwrap().starts_with(b"\x89PNG"));
    assert_eq!(runs[2].out, "", "nothing written");
}

#[test]
fn a_bad_script_or_a_bad_flag_gets_its_exit_code() {
    let dir = tempfile::tempdir().unwrap();
    let script = dir.path().join("f.js");
    std::fs::write(&script, "box(0, 0, 10, 10);\ncircle(0, 0, -1);\n").unwrap();
    let script = script.to_str().unwrap();
    let runs = cli_each(&[
        &["canvas", "create", "main"],
        &["canvas", "draw", "main", script, "--name", "f", "--at", "0,0"],
        &["canvas", "draw", "main", script, "--name", "f"],
        &["canvas", "draw", "main", script, "--name", "f", "--at", "0,0", "--beside", "i1"],
        &["canvas", "draw", "main", script, "--name", "f", "--at", "zero"],
        &["canvas", "draw", "main", script, "--name", "f", "--at", "0,0", "--data", "{nope"],
        &["canvas", "draw", "main", "/nonexistent/f.js", "--name", "f", "--at", "0,0"],
    ]);
    let codes: Vec<u8> = runs.iter().map(|r| r.code).collect();
    assert_eq!(
        codes,
        [
            exit::OK,
            exit::INVALID,
            exit::USAGE,
            exit::USAGE,
            exit::USAGE,
            exit::USAGE,
            exit::NOT_FOUND
        ]
    );
    assert!(runs[1].err.contains("script:2:1: circle"), "{}", runs[1].err);
}
