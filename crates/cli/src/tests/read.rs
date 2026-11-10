//! `slinger canvas read` against a real daemon.

use super::{cli_each, json};
use crate::output::exit;

/// A stroke at `x` (tenths of a unit).
fn stroke(x: i64) -> String {
    format!(
        r#"{{"op":"add","item":{{"page":"p1","kind":"stroke","width":20,"pts":[{x},0,100,50,100,-50]}}}}"#
    )
}

#[test]
fn read_prints_the_packet_and_after_keeps_only_new_ink() {
    let out = tempfile::tempdir().unwrap();
    let dir = out.path().join("packet");
    let dir = dir.to_str().unwrap();
    let (first, second) = (stroke(0), stroke(30_000));
    let runs = cli_each(&[
        &["canvas", "create", "idea"],
        &["canvas", "apply", "idea", &first],
        &["canvas", "read", "idea"],
        &["canvas", "apply", "idea", &second],
        &["canvas", "read", "idea", "--after", "1", "--page", "Page 1", "--out", dir],
        &["canvas", "read", "idea", "--after", "2", "--select", "c2,i1", "--json"],
    ]);
    for r in &runs {
        assert_eq!((r.code, r.err.as_str()), (exit::OK, ""), "{}", r.out);
    }
    let all = &runs[2].out;
    assert!(all.starts_with("slinger-packet 1 canvas=idea page=Page%201 from=1 to=1 "), "{all}");
    assert!(all.contains("\n# new\nc1 ink "), "{all}");
    assert!(all.ends_with("# page\n"), "{all}");

    let since = &runs[4].out;
    assert!(since.contains(&format!(" at={dir}\n")), "{since}");
    assert!(since.contains("\n# new\nc2 ink ") && since.contains("\n# page\nc1 ink "), "{since}");
    assert!(std::path::Path::new(dir).join("c2.png").exists());

    let doc = json(&runs[5]);
    assert_eq!(doc["command"], "canvas read");
    assert!(doc["packet"].as_str().unwrap().starts_with("sha256:"));
    assert!(doc["text"].as_str().unwrap().contains(" from=3 to=2 "));
    assert_eq!(doc["images"], serde_json::json!([]));
    assert!(doc["dir"].is_string());
}

#[test]
fn a_missing_canvas_or_a_bad_id_gets_its_exit_code() {
    let runs = cli_each(&[
        &["canvas", "create", "idea"],
        &["canvas", "read", "gone"],
        &["canvas", "read", "idea", "--select", "zz"],
    ]);
    assert_eq!(runs[1].code, exit::NOT_FOUND);
    assert_eq!(runs[2].code, exit::INVALID);
    assert!(runs[2].err.contains("c42"), "{}", runs[2].err);
}
