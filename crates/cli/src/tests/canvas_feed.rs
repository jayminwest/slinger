//! `slinger canvas changes` against a real daemon.

use super::{cli_each, json};
use crate::output::exit;

const BOX: &str = r#"{"op":"add","item":{"page":"p1","kind":"box","x":10,"y":20,"w":100,"h":50}}"#;

#[test]
fn changes_prints_the_operations_after_a_number() {
    let runs = cli_each(&[
        &["canvas", "create", "main"],
        &["canvas", "apply", "main", BOX],
        &["canvas", "apply", "main", BOX],
        &["canvas", "changes", "main"],
        &["canvas", "changes", "main", "--after", "1", "--json"],
        &["canvas", "changes", "main", "--after", "2", "--wait", "10"],
        &["canvas", "changes", "main", "--after", "3"],
    ]);
    for r in &runs[..6] {
        assert_eq!((r.code, r.err.as_str()), (exit::OK, ""), "{}", r.out);
    }
    let lines: Vec<serde_json::Value> =
        runs[3].out.lines().map(|line| serde_json::from_str(line).unwrap()).collect();
    assert_eq!(
        (lines.len(), &lines[0]["n"], &lines[1]["item"]["id"]),
        (2, &1.into(), &"i2".into())
    );
    let doc = json(&runs[4]);
    assert_eq!((&doc["command"], &doc["next"]), (&"canvas changes".into(), &3.into()));
    assert_eq!(doc["ops"][0]["n"], 2);
    assert_eq!(runs[5].out, "");
    assert_eq!(runs[6].code, exit::INVALID);
}
