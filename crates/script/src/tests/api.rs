//! The API as an agent meets it (docs/design/figures.md, Appendix A):
//! each call's shape in tenths, the options every call takes, colours,
//! nested defaults, the data, and errors at the call by name.

use serde_json::{Value, json};

use super::{drawn, failed, kind, view};
use crate::{COLOURS, View, run};

pub(super) fn one(script: &str) -> Value {
    let d = drawn(script);
    assert_eq!(d.items.len(), 1, "{:?}", d.items);
    d.items[0].clone()
}

#[test]
fn each_call_draws_an_add_item_object_in_tenths() {
    assert_eq!(
        one("fig.box(1.25, -1.25, 10, 5)"),
        json!({"kind": "box", "color": "#8a5cf5",
        "x": 13, "y": -13, "w": 100, "h": 50, "locked": true, "fig": "fig"})
    );
    assert_eq!(
        one("fig.line(0, 0, 10, 20, {locked: false})"),
        json!({"kind": "arrow", "color": "#8a5cf5",
        "pts": [0, 0, 100, 200], "heads": "none", "fig": "fig"})
    );
    assert_eq!(one("fig.arrow(0, 0, 10, 0, {heads: 'both'})")["heads"], "both");
    assert_eq!(one("fig.arrow(0, 0, 10, 0)").get("heads"), None);
    assert_eq!(
        one("fig.path([[0, 0], [1, 1], [3, 0]], {width: 3})"),
        json!({"kind": "stroke", "color": "#8a5cf5",
        "width": 30, "pts": [0, 0, 10, 10, 20, -10], "locked": true, "fig": "fig"})
    );
    assert_eq!(one("fig.path([[0, 0], [1, 1], [3, 0]], {closed: true})")["closed"], true);
    assert_eq!(
        one("fig.text('hi', 5, 6, {size: 20})"),
        json!({"kind": "text", "color": "#8a5cf5",
        "x": 50, "y": 60, "w": 200, "h": 200, "text": "hi", "size": 200, "locked": true, "fig": "fig"})
    );
    let c = one("fig.circle(0, 0, 10, {fill: 'red'})");
    assert_eq!((c["closed"].clone(), c["fill"].clone()), (json!(true), json!("#ff000033")));
    assert_eq!(c["pts"].as_array().map(Vec::len), Some(96));
    let e = one("fig.ellipse(0, 0, 20, 10)");
    assert_eq!(
        (e["closed"].clone(), e["pts"][0].clone(), e["pts"][1].clone()),
        (json!(true), json!(200), json!(0))
    );
    let d = drawn("warn(JSON.stringify(fig.ellipse(5, 5, 20, 10)));");
    assert_eq!(d.warnings[0].message, r#"{"x":-15,"y":-5,"w":40,"h":20}"#, "its bounds are exact");
}

#[test]
fn width_draws_lines_paths_and_arrows_at_that_width() {
    // The canvas draws an arrow 2 wide: a line of another width is a stroke.
    assert_eq!(one("fig.line(0, 0, 10, 0, {width: 2})")["kind"], "arrow");
    let wide = one("fig.line(0, 0, 10, 0, {width: 4, dash: 'dashed'})");
    assert_eq!(
        wide,
        json!({"kind": "stroke", "color": "#8a5cf5", "width": 40, "pts": [0, 0, 100, 0],
        "dash": "dashed", "locked": true, "fig": "fig"})
    );
    assert_eq!(one("fig.arrow(0, 0, 10, 0, {width: 5, heads: 'none'})")["width"], 50);
    let e = failed("fig.arrow(0, 0, 10, 0, {width: 5})");
    assert!(e.message.starts_with("fig.arrow: width must be 2 on an arrow with heads"), "{e}");
}

#[test]
fn a_text_aligns_on_x_and_wraps_to_w() {
    let x = |s: &str| one(s)["x"].as_i64();
    assert_eq!(x("fig.text('abcd', 100, 0)"), Some(1000), "left: x is its left edge");
    assert_eq!(x("fig.text('abcd', 100, 0, {align: 'center'})"), Some(1000 - 240), "4 cells of 12");
    assert_eq!(x("fig.text('abcd', 100, 0, {align: 'right'})"), Some(1000 - 480));
    let wrapped = one("fig.text('one two three', 0, 0, {w: 60})");
    assert_eq!((wrapped["w"].clone(), wrapped["h"].clone()), (json!(600), json!(720)));
    let e = failed("fig.text('a', 0, 0, {align: 'middle'})");
    assert!(e.message.starts_with(r#"fig.text: align must be "left", "center" or "right""#), "{e}");
}

#[test]
fn a_box_label_sits_inside_at_the_top_left_and_a_slot_is_dashed_or_dotted() {
    let d =
        drawn("fig.box(10, 20, 300, 100, {label: 'Why is A more?', name: 'why', color: 'blue'});");
    assert_eq!(d.items.len(), 2);
    let l = &d.items[1];
    assert_eq!((kind(l), &l["text"], &l["size"]), ("text", &json!("Why is A more?"), &json!(200)));
    assert_eq!(
        (&l["x"], &l["y"], &l["w"]),
        (&json!(200), &json!(300), &json!(2800)),
        "10 in, wrapped to the box"
    );
    assert_eq!(
        (&l["part"], &l["color"]),
        (&json!("why"), &json!("#0000ff")),
        "the label is the box's"
    );
    assert_eq!(
        drawn("fig.box(0, 0, 10, 10, {label: ''})").items.len(),
        1,
        "an empty label is none"
    );
    assert_eq!(drawn("fig.box(0, 0, 300, 100, {label: 'big', size: 40})").items[1]["size"], 400);
    assert_eq!(one("fig.slot(0, 0, 10, 10)")["dash"], "dashed");
    assert_eq!(one("fig.slot(0, 0, 10, 10, {by: 'agent'})")["dash"], "dotted");
    let s = drawn("fig.slot(0, 0, 300, 100, {label: 'yours', name: 'a'})");
    assert_eq!(
        (s.items[0]["part"].clone(), s.items[1]["text"].clone()),
        (json!("a"), json!("yours"))
    );
    let e = failed("fig.slot(0, 0, 10, 10, {by: 'me'})");
    assert!(e.message.starts_with(r#"fig.slot: by must be "user" or "agent""#), "{e}");
}

#[test]
fn text_is_measured_by_the_cores_rule() {
    let d = drawn(
        "const m = measure('hello world', {w: 60});\nconst n = measure('hello world');\nwarn(JSON.stringify([m, n]));",
    );
    // "hello" fills five cells; the space after it has a row of its own, as in nvim.
    assert_eq!(d.warnings[0].message, r#"[{"w":60,"h":72},{"w":132,"h":24}]"#);
    assert_eq!(d.warnings[0].line, 3);
    assert_eq!(
        drawn("warn(JSON.stringify(measure('hello', {size: 24})))").warnings[0].message,
        r#"{"w":60,"h":24}"#
    );
}

#[test]
fn colours_take_names_and_hex_and_an_unknown_one_lists_the_names() {
    for (name, hex) in [
        ("series3", "#009e73"),
        ("good", "#2f9e44"),
        ("bad", "#e5484d"),
        ("warn", "#c77700"),
        ("accent", "#4c9aff"),
        ("muted", "#8c8c8c"),
    ] {
        assert_eq!(
            one(&format!("fig.box(0, 0, 1, 1, {{color: '{name}'}})"))["color"],
            hex,
            "{name}"
        );
    }
    assert_eq!(one("fig.box(0, 0, 1, 1, {color: '#ABCDEFff'})")["color"], "#abcdef");
    assert_eq!(one("fig.box(0, 0, 1, 1, {fill: '#11223344'})")["fill"], "#11223344");
    assert_eq!(one("fig.box(0, 0, 1, 1, {color: 'blue', fill: true})")["fill"], "#0000ff33");
    let e = failed("\n  fig.box(0, 0, 1, 1, {color: 'teal'})");
    assert_eq!((e.line, e.col), (2, 7));
    assert!(e.message.starts_with("fig.box: color must be"), "{e}");
    assert!(COLOURS.iter().all(|(name, _)| e.message.contains(name)), "{e}");
    let e = failed("fig.color = 'nope';\nfig.box(0, 0, 1, 1);");
    assert!(e.message.contains("fig.color"), "{e}");
}

#[test]
fn defaults_nest_and_the_nearest_wins() {
    let d = drawn(
        "fig.color = 'red';
         fig.box(0, 0, 1, 1);
         const inner = fig.part('p', {color: 'green', dash: 'dotted'}, () => {
           fig.box(0, 0, 1, 1, {dash: 'solid'}); fig.text('a', 0, 0); fig.line(0, 0, 1, 1);
         });
         const b = fig.box(0, 0, 1, 1);
         fig.group(b, fig.box(5, 5, 1, 1, {color: 'ink'}), {color: 'blue', fill: true, dash: 'dashed'});
         fig.color = 'accent';
         fig.box(0, 0, 1, 1);",
    );
    let colour: Vec<&str> = d.items.iter().filter_map(|i| i["color"].as_str()).collect();
    assert_eq!(
        colour,
        ["#ff0000", "#008000", "#008000", "#008000", "#0000ff", "#000000", "#4c9aff"]
    );
    assert_eq!(d.items[1].get("dash"), None, "its own solid wins");
    assert_eq!(d.items[2].get("dash"), None, "a text takes no dash");
    assert_eq!(d.items[3]["dash"], "dotted");
    assert_eq!(
        (d.items[4]["fill"].clone(), d.items[4]["dash"].clone()),
        (json!("#0000ff33"), json!("dashed"))
    );
    assert_eq!(d.items[5]["fill"], "#00000033", "a tint of its own colour");
    assert_eq!(d.items.iter().filter(|i| i["part"] == "p").count(), 3);
}

#[test]
fn a_part_is_named_by_an_option_by_a_handle_or_by_a_function() {
    let d = drawn(
        "fig.box(0, 0, 1, 1, {name: 'a'});
         fig.box(0, 0, 1, 1).name('b');
         fig.part('c', () => { fig.box(0, 0, 1, 1); fig.box(2, 0, 1, 1, {name: 'd'}); }).moveBy(10, 0);
         fig.group(fig.box(0, 0, 1, 1), fig.box(3, 0, 1, 1, {name: 'f'})).name('e');",
    );
    let parts: Vec<&str> = d.items.iter().filter_map(|i| i["part"].as_str()).collect();
    assert_eq!(parts, ["a", "b", "c", "d", "e", "f"], "a shape's own name wins");
    assert_eq!(
        (d.items[2]["x"].clone(), d.items[3]["x"].clone()),
        (json!(100), json!(120)),
        "fig.part returns what it drew"
    );
}

#[test]
fn the_script_reads_its_data_as_the_global_data() {
    let said = |data: Value| {
        let d = run("warn(JSON.stringify([data, fig.name, fig.color]));", &data, &view());
        d.map(|d| d.warnings[0].message.clone()).unwrap_or_default()
    };
    assert_eq!(said(json!({"n": 1})), r##"[{"n":1},"fig","#8a5cf5"]"##);
    assert_eq!(said(Value::Null), r##"[null,"fig","#8a5cf5"]"##);
    let e = failed("fig.data.n");
    assert!(e.message.contains("TypeError"), "fig.data is gone: {e}");
    let v = View { target: Some([500.0, 600.0, 300.0, 200.0]), ..view() };
    let d = run("warn(JSON.stringify(fig.view));", &json!(null), &v);
    let seen = d.map(|d| d.warnings[0].message.clone()).unwrap_or_default();
    assert_eq!(seen, r#"{"target":{"x":0,"y":0,"w":300,"h":200},"page":"p1"}"#);
}

#[test]
fn bad_arguments_fail_at_the_call_by_name() {
    for (script, said) in [
        ("fig.box(0, 0, 'wide', 10)", r#"fig.box: w must be a number above 0, got "wide""#),
        (
            "fig.box(0, 0, 10, 10, {colour: 'red'})",
            "fig.box: unknown option colour; fig.box takes color, fill, dash, name, locked, label, size",
        ),
        ("fig.text('', 0, 0)", "fig.text: text must be a non-empty string"),
        ("fig.text(0, 0, 'a')", r#"fig.text: y must be a number, got "a""#),
        ("fig.line(0, 0, 5, 5, {fill: true})", "fig.line: unknown option fill"),
        (
            "fig.path([[0, 0], [1, 1]], {fill: true})",
            "fig.path: fill takes a box, a circle, an ellipse and a path with closed: true",
        ),
        (
            "fig.path([[0, 0], ['ten', 5]])",
            "fig.path: points[1] must be [x, y] or a handle, got an array",
        ),
        ("fig.text('a', 0, 0, {dash: 'dashed'})", "fig.text: unknown option dash"),
        ("fig.text('a', 0, 0, {size: 2})", "fig.text: size must be a number from 4 to 400"),
        ("fig.arrow([0, 0])", "fig.arrow: takes (x1, y1, x2, y2, opts?) or (from, to, opts?)"),
        ("fig.arrow([0, 0], 'b')", "fig.arrow: to must be [x, y] or a handle"),
        ("fig.box(0, 0, 1, 1, {name: 'A b'})", "fig.box: a part name is 1 to 40 of a-z, 0-9 and -"),
        ("fig.part('A b', () => 1)", "fig.part: a part name is 1 to 40 of a-z, 0-9 and -"),
        (
            "fig.part('a', () => fig.part('b', () => 1))",
            "fig.part: parts do not nest: b is inside a",
        ),
        ("fig.part('a', 3)", "fig.part: the last argument must be a function"),
        (
            "fig.group({}, () => 1)",
            "fig.group: takes handles, not a function; to set defaults for what a function draws, use fig.part",
        ),
        ("fig.group(3)", "fig.group: each argument must be a handle a draw call returned, got 3"),
        ("fig.stack(fig.box(0, 0, 1, 1))", "fig.stack: takes an array of handles first"),
        ("fig.plot(0, 0, 10, 10, {xRange: [1, 1]})", "fig.plot: xRange must be [min, max]"),
        ("fig.plot(0, 0, 10, 10, {frame: 'wavy'})", "fig.plot: frame must be"),
        ("fig.table([['a']], 0, 0).cell(1, 0)", "t.cell: (1, 0) is not a cell"),
        ("fig.box(0, 0, 1, 1, {dash: 'wavy'})", "fig.box: dash must be"),
        ("fig.box(0, 0, 1, 1).below('x')", "below: other must be a handle or {x, y, w, h}"),
        ("undefinedThing()", "ReferenceError: undefinedThing is not defined"),
        ("box(0, 0, 1, 1)", "ReferenceError: box is not defined"),
    ] {
        let e = failed(script);
        assert!(e.message.starts_with(said) || e.message.contains(said), "{script}: {e}");
        assert_eq!(e.line, 1, "{script}: {e}");
    }
    let e = failed("function helper() {\n  return fig.box(0, 0, -1, 1);\n}\nhelper();");
    assert_eq!((e.line, e.col), (2, 14), "the line of the call, not of the caller: {e}");
    let e = failed("fig.part('a', () => {\n  fig.box(0, 0, 1, 1);\n  fig.box(0, 0, -1, 1);\n});");
    assert_eq!(e.line, 3, "a call inside a part's function names its own line: {e}");
    let e = failed("let x = ;");
    assert!(e.message.starts_with("SyntaxError"), "{e}");
    assert_eq!(e.line, 1);
}
