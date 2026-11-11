//! `slinger pane ...` against a real daemon.

use super::{cli_each, json};
use crate::output::exit;

#[test]
fn open_list_and_close_print_their_results() {
    let runs = cli_each(&[
        &["workspace", "open", "--name", "notes"],
        &["canvas", "create", "board"],
        &["pane", "open", "notes", "--type", "canvas", "--target", "board"],
        &[
            "pane", "open", "notes", "--type", "canvas", "--target", "notes", "--beside", "2",
            "--below",
        ],
        &["pane", "list", "notes"],
        &["pane", "close", "notes", "1"],
        &["pane", "close", "notes", "9"],
        &["pane", "resize", "notes", "1", "0.3"],
        &["pane", "list", "notes", "--json"],
    ]);
    for r in &runs {
        assert_eq!((r.code, r.err.as_str()), (exit::OK, ""), "{}", r.out);
    }
    let out: Vec<&str> = runs.iter().map(|r| r.out.as_str()).collect();
    assert_eq!(
        out[2..6],
        ["2\n", "3\n", "1\tcanvas\tnotes\n2\tcanvas\tboard\n3\tcanvas\tnotes\n", ""]
    );
    assert_eq!(out[6..8], ["", ""], "a pane already gone; a resize");
    let doc = json(&runs[8]);
    assert_eq!(doc["command"], "pane list");
    assert_eq!(doc["workspace"], "notes");
    assert_eq!(doc["layout"]["split"], "column");
    assert_eq!(doc["layout"]["ratio"], 0.3);
    assert_eq!(
        doc["panes"],
        serde_json::json!([
            { "pane": 1, "kind": "canvas", "target": "board" },
            { "pane": 2, "kind": "canvas", "target": "notes" },
        ])
    );
}

#[test]
fn missing_things_and_bad_types_get_their_exit_codes() {
    let runs = cli_each(&[
        &["pane", "list", "nope"],
        &["workspace", "open", "--name", "w"],
        &["pane", "open", "w", "--type", "ui", "--target", "board"],
        &["pane", "open", "w", "--type", "canvas", "--target", "w", "--beside", "4"],
        &["pane", "open", "w", "--type", "browser", "--target", "x"],
        &["pane", "resize", "w", "1", "half"],
        &["pane", "resize", "nope", "1", "0.5"],
    ]);
    let codes: Vec<u8> = runs.iter().map(|r| r.code).collect();
    assert_eq!(
        codes,
        [
            exit::NOT_FOUND,
            exit::OK,
            exit::NOT_FOUND,
            exit::NOT_FOUND,
            exit::USAGE,
            exit::USAGE,
            exit::NOT_FOUND
        ]
    );
}
