//! The two error codes' kinds, the source line and the stack an error
//! carries, and what the script reads of the canvas round its figure.

use serde_json::{Value, json};

use super::{drawn, failed, view};
use crate::{Anchor, Fault, ITEMS, Near, View, run};

#[test]
fn a_throw_carries_its_source_line_and_the_calls_it_was_in() {
    let e = failed("function row(y) {\n  fig.box(0, y, 'wide', 10);\n}\nrow(5);\n");
    assert_eq!((e.kind, e.line, e.col), (Fault::Script, 2, 14), "{e}");
    assert_eq!(e.shown, "  fig.box(0, y, 'wide', 10);");
    assert_eq!(e.stack, ["row (script:2:14)", "top level (script:4:1)"]);
    assert_eq!(
        e.report(),
        "script:2:14: fig.box: w must be a number above 0, got \"wide\"\n  \
         2 |   fig.box(0, y, 'wide', 10);\n    |              ^\n  \
         at row (script:2:14)\n  at top level (script:4:1)"
    );
}

#[test]
fn a_throw_at_the_top_level_shows_its_line_and_no_stack() {
    let e = failed("fig.box(0, 0, 1, 1);\nthrow new Error('no');\n");
    assert_eq!(
        e.report(),
        "script:2:11: Error: no\n  2 | throw new Error('no');\n    |           ^"
    );
}

#[test]
fn a_long_source_line_is_cut_round_the_column() {
    let script = format!("const a = '{}'; fig.box(0, 0, -1, 1);", "x".repeat(300));
    let e = failed(&script);
    assert!(e.shown.starts_with('…') && e.shown.contains("fig.box(0, 0, -1, 1)"), "{}", e.shown);
    assert!(e.shown.chars().count() <= 122, "{}", e.shown);
    assert!(!e.report().contains('^'), "no mark under a cut line");
}

#[test]
fn each_limit_is_a_limit_and_a_fault_is_a_script() {
    for script in [
        "while (true) {}",
        "const a = [];\nfor (;;) a.push(new Array(100000).fill(1));",
        "function f() { return f() + 1; }\nf();",
        "for (let i = 0; i < 6000; i += 1) fig.box(i, 0, 1, 1);",
    ] {
        assert_eq!(failed(script).kind, Fault::Limit, "{script}");
    }
    let long = format!("// {}\n", "x".repeat(crate::SCRIPT_BYTES));
    assert_eq!(failed(&long).kind, Fault::Limit);
    for script in ["fig.box(0, 0);", "nope();", "let = ;", "throw 'x';"] {
        assert_eq!(failed(script).kind, Fault::Script, "{script}");
    }
}

#[test]
fn too_many_shapes_is_a_limit_even_when_the_script_catches_it() {
    let e = failed(
        "for (let i = 0; i < 6000; i += 1) {\n  try { fig.box(i, 0, 1, 1); } catch (e) {}\n}\n",
    );
    assert_eq!(e.kind, Fault::Limit);
    assert_eq!(e.message, format!("a figure holds at most {ITEMS} shapes"));
}

fn seen(view: &View) -> Value {
    let d = run("warn(JSON.stringify({anchor, around}));", &json!({}), view).unwrap();
    serde_json::from_str(&d.warnings[0].message).unwrap()
}

#[test]
fn a_script_reads_its_anchor_and_the_items_around_it() {
    assert_eq!(seen(&view()), json!({"anchor": null, "around": []}));
    let anchor =
        Anchor { id: "i3".into(), kind: "text".into(), w: 60.0, h: 24.0, text: Some("hi".into()) };
    let near = Near { id: "i9".into(), kind: "stroke".into(), x: -5.0, y: 10.0, w: 20.0, h: 2.5 };
    let v = View { anchor: Some(anchor), around: vec![near], ..view() };
    assert_eq!(
        seen(&v),
        json!({
            "anchor": {"id": "i3", "kind": "text", "w": 60, "h": 24, "text": "hi"},
            "around": [{"id": "i9", "kind": "stroke", "x": -5, "y": 10, "w": 20, "h": 2.5}],
        })
    );
    let d = drawn("fig.box(0, 0, anchor === null ? 10 : 20, 10);");
    assert_eq!(d.items.len(), 1);
}

#[test]
fn anchor_and_around_are_read_only() {
    let v = View {
        anchor: Some(Anchor { id: "b1".into(), kind: "box".into(), ..Anchor::default() }),
        ..view()
    };
    for script in [
        "'use strict'; anchor.w = 5;",
        "'use strict'; around.push(1);",
        "'use strict'; anchor = 1;",
    ] {
        assert!(run(script, &json!({}), &v).is_err(), "{script}");
    }
}
