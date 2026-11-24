//! The owner's softmax worksheet (docs/design/figures.md, Appendix B,
//! copied unchanged into softmax.js) runs on the API as designed; the
//! redraw worksheet (Appendix D): 56 items in 15 parts, and the redraw's
//! data fills two parts by name.

use std::collections::BTreeSet;

use serde_json::{Value, json};

use super::{kind, view};
use crate::{Drawn, run};

const SCRIPT: &str = include_str!("worksheet.js");
const SOFTMAX: &str = include_str!("softmax.js");
const DATA: &str = include_str!("worksheet.json");

fn worksheet(patch: &Value) -> Drawn {
    let mut data: Value = serde_json::from_str(DATA).unwrap_or_default();
    if let (Some(d), Some(p)) = (data.as_object_mut(), patch.as_object()) {
        d.extend(p.iter().map(|(k, v)| (k.clone(), v.clone())));
    }
    match run(SCRIPT, &data, &view()) {
        Ok(d) => d,
        Err(e) => unreachable!("the worksheet failed: {e}"),
    }
}

fn count(d: &Drawn, k: &str) -> usize {
    d.items.iter().filter(|i| kind(i) == k).count()
}

#[test]
fn the_worksheet_draws_56_items_in_15_parts() {
    let d = worksheet(&json!({}));
    assert_eq!(d.items.len(), 56);
    assert_eq!(
        (count(&d, "text"), count(&d, "box"), count(&d, "arrow"), count(&d, "stroke")),
        (33, 17, 4, 2)
    );
    let lines = d.items.iter().filter(|i| i["heads"] == "none").count();
    assert_eq!(lines, 3, "the name line and the plot's two axes have no heads");
    let parts: BTreeSet<&str> = d.items.iter().filter_map(|i| i["part"].as_str()).collect();
    assert_eq!(parts.len(), 15, "{parts:?}");
    assert!(d.items.iter().all(|i| i["fig"] == "fig" && i["locked"] == true));
    assert!(d.warnings.is_empty(), "{:?}", d.warnings);
}

#[test]
fn the_worksheet_is_styled_as_the_script_says() {
    let d = worksheet(&json!({}));
    let part = |p: &'static str| d.items.iter().filter(move |i| i["part"] == p);
    let answer: Vec<&Value> = part("1a-answer").collect();
    assert_eq!(answer.len(), 1);
    assert_eq!(
        answer[0],
        &json!({"kind": "box", "color": "#8a5cf5", "x": 6000, "y": 2500, "w": 3000, "h": 900,
        "dash": "dashed", "locked": true, "fig": "fig", "part": "1a-answer"})
    );
    assert!(part("feedback").all(|i| i["dash"] == "dotted"));
    let header: Vec<&Value> = part("q2-table").filter(|i| i["fill"].is_string()).collect();
    assert_eq!(header.len(), 6, "the header row is tinted");
    assert!(header.iter().all(|i| i["fill"] == "#8a5cf533"));
    let muted = d.items.iter().filter(|i| i["color"] == "#8c8c8c").count();
    assert_eq!(muted, 2 + 9 + 1, "two notes, nine tick labels and 'Actual top 5'");
    let curve: Vec<&Value> = part("q1-plot").filter(|i| kind(i) == "stroke").collect();
    assert_eq!(curve[0]["color"], "#2f80ed", "the first line of a plot is series1");
    assert_eq!(curve[0]["pts"].as_array().map(Vec::len), Some(2 * 41), "steps: 40 is 41 points");
}

#[test]
fn the_redraw_data_fills_two_parts_by_name() {
    let before = worksheet(&json!({}));
    let after = worksheet(&json!({
        "actual": ["C", "A", "E", "D", "B"],
        "feedback": "1a and 2 are right. In 1b, read where the curve crosses 200: about 9 years.",
    }));
    assert_eq!(after.items.len(), 62);
    let texts =
        |d: &Drawn, p: &str| d.items.iter().filter(|i| i["part"] == p && kind(i) == "text").count();
    assert_eq!((texts(&after, "actual-top-5"), texts(&after, "feedback")), (5, 1));
    let check =
        |d: &Drawn| d.items.iter().find(|i| i["part"] == "check").cloned().unwrap_or_default();
    assert_eq!(check(&before).get("fill"), None);
    assert_eq!(check(&after)["fill"], "#009e73", "good, as given");
    let others = |d: &Drawn| -> Vec<Value> {
        d.items
            .iter()
            .filter(|i| !matches!(i["part"].as_str(), Some("actual-top-5" | "feedback" | "check")))
            .cloned()
            .collect()
    };
    assert_eq!(others(&before), others(&after), "every other shape is the same");
}

#[test]
fn the_same_script_and_data_give_the_same_bytes() {
    let a = worksheet(&json!({}));
    let b = worksheet(&json!({}));
    assert_eq!(serde_json::to_string(&a.items).ok(), serde_json::to_string(&b.items).ok());
}

#[test]
fn the_owners_softmax_worksheet_runs_unchanged() {
    let d = match run(SOFTMAX, &Value::Null, &view()) {
        Ok(d) => d,
        Err(e) => unreachable!("Appendix B failed: {e}"),
    };
    // Appendix B says 56 items; as the API is written it draws 53: the
    // spike's prelude is not in the repository and drew three more.
    assert_eq!(d.items.len(), 53);
    assert_eq!((count(&d, "text"), count(&d, "box")), (35, 18));
    let parts: BTreeSet<&str> = d.items.iter().filter_map(|i| i["part"].as_str()).collect();
    let named = [
        "given",
        "p1a",
        "p1a-actual",
        "p1b",
        "p1c",
        "p2-92",
        "p2a",
        "p2a-why",
        "p2b",
        "p2b-why",
        "p2c",
        "p2c-why",
        "p3a",
        "p3a-actual",
        "p3b",
    ];
    assert_eq!(parts, BTreeSet::from(named), "15 named parts");
    assert!(d.warnings.is_empty(), "{:?}", d.warnings);
    let part = |p: &str| d.items.iter().filter(|i| i["part"] == p).cloned().collect::<Vec<_>>();
    let given = part("given");
    assert_eq!(given.len(), 1 + 3 + 3, "its dotted frame, three bars and their labels");
    assert_eq!(given[0]["dash"], "dotted");
    let answer = part("p1a");
    assert_eq!(
        (answer.len(), answer[0]["dash"].clone()),
        (2, json!("dashed")),
        "a dashed box and its label"
    );
    assert_eq!(part("p1a-actual")[0]["dash"], "dotted");
    assert_eq!(part("p2-92").len(), 1, "an empty label draws nothing");
}
