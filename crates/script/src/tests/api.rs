//! The API as an agent meets it: shapes, colours, nested defaults, errors
//! at the call, and warnings.

use serde_json::{Value, json};

use super::{drawn, failed, kind, view};
use crate::{COLOURS, View, run};

fn one(script: &str) -> Value {
    let d = drawn(script);
    assert_eq!(d.items.len(), 1, "{:?}", d.items);
    d.items[0].clone()
}

#[test]
fn each_shape_is_an_add_item_object_in_tenths() {
    assert_eq!(
        one("box(1.25, -1.25, 10, 5)"),
        json!({"kind": "box", "color": "#8a5cf5",
        "x": 13, "y": -13, "w": 100, "h": 50, "locked": true, "fig": "fig"})
    );
    assert_eq!(
        one("line(0, 0, 10, 20, {locked: false})"),
        json!({"kind": "arrow", "color": "#8a5cf5",
        "pts": [0, 0, 100, 200], "heads": "none", "fig": "fig"})
    );
    assert_eq!(one("arrow(0, 0, 10, 0, {heads: 'both'})")["heads"], "both");
    assert_eq!(one("arrow(0, 0, 10, 0)").get("heads"), None);
    assert_eq!(
        one("poly([[0, 0], [1, 1], [3, 0]], {width: 3})"),
        json!({"kind": "stroke", "color": "#8a5cf5",
        "width": 30, "pts": [0, 0, 10, 10, 20, -10], "locked": true, "fig": "fig"})
    );
    assert_eq!(
        one("text(5, 6, 'hi', {size: 20})"),
        json!({"kind": "text", "color": "#8a5cf5",
        "x": 50, "y": 60, "w": 200, "h": 200, "text": "hi", "size": 200, "locked": true, "fig": "fig"})
    );
    let c = one("circle(0, 0, 10, {fill: 'red'})");
    assert_eq!((c["closed"].clone(), c["fill"].clone()), (json!(true), json!("#ff000033")));
    assert_eq!(c["pts"].as_array().map(Vec::len), Some(96));
}

#[test]
fn text_is_measured_by_the_cores_rule() {
    let d = drawn(
        "const m = measure('hello world', {w: 60});\nconst n = measure('hello world');\nwarn(JSON.stringify([m, n]));",
    );
    // "hello" fills five cells; the space after it has a row of its own, as in nvim.
    assert_eq!(d.warnings[0].message, r#"[{"w":60,"h":72},{"w":132,"h":24}]"#);
    assert_eq!(d.warnings[0].line, 3);
}

#[test]
fn colours_take_names_and_hex_and_an_unknown_one_lists_the_names() {
    assert_eq!(one("box(0, 0, 1, 1, {color: 'series3'})")["color"], "#009e73");
    assert_eq!(one("box(0, 0, 1, 1, {color: '#ABCDEFff'})")["color"], "#abcdef");
    assert_eq!(one("box(0, 0, 1, 1, {fill: '#11223344'})")["fill"], "#11223344");
    assert_eq!(one("box(0, 0, 1, 1, {color: 'blue', fill: true})")["fill"], "#0000ff33");
    let e = failed("\n  box(0, 0, 1, 1, {color: 'teal'})");
    assert_eq!((e.line, e.col), (2, 3));
    assert!(e.message.starts_with("box: color must be"), "{e}");
    assert!(COLOURS.iter().all(|(name, _)| e.message.contains(name)), "{e}");
    let e = failed("fig.color = 'nope';\nbox(0, 0, 1, 1);");
    assert!(e.message.contains("fig.color"), "{e}");
}

#[test]
fn defaults_nest_and_the_nearest_wins() {
    let d = drawn(
        "fig.color = 'red';
         box(0, 0, 1, 1);
         group({color: 'blue', dash: 'dotted', fill: true}, () => {
           box(0, 0, 1, 1);
           part('p', {color: 'green'}, () => { box(0, 0, 1, 1, {dash: 'solid'}); text(0, 0, 'a'); line(0, 0, 1, 1); });
         });",
    );
    let colour: Vec<&str> = d.items.iter().filter_map(|i| i["color"].as_str()).collect();
    assert_eq!(colour, ["#ff0000", "#0000ff", "#008000", "#008000", "#008000"]);
    assert_eq!(d.items[1]["dash"], "dotted");
    assert_eq!(d.items[1]["fill"], "#0000ff33");
    assert_eq!(d.items[2].get("dash"), None, "its own solid wins");
    assert_eq!(d.items[2]["fill"], "#00800033", "a tint of its own colour");
    assert_eq!(
        (d.items[3].get("dash"), d.items[3].get("fill")),
        (None, None),
        "a text takes neither"
    );
    assert_eq!((d.items[4]["dash"].clone(), d.items[4].get("fill")), (json!("dotted"), None));
    assert_eq!(d.items.iter().filter(|i| i["part"] == "p").count(), 3);
}

#[test]
fn bad_arguments_fail_at_the_call_by_name() {
    for (script, said) in [
        ("box(0, 0, 'wide', 10)", r#"box: w must be a number above 0, got "wide""#),
        (
            "box(0, 0, 10, 10, {colour: 'red'})",
            "box: unknown option colour; box takes color, fill, dash, locked",
        ),
        ("text(0, 0, '')", "text: text must be a non-empty string"),
        ("line(0, 0, 5, 5, {fill: true})", "line: unknown option fill"),
        (
            "poly([[0, 0], [1, 1]], {fill: true})",
            "poly: fill takes box, circle and poly with closed: true",
        ),
        ("text(0, 0, 'a', {dash: 'dashed'})", "text: unknown option dash"),
        ("text(0, 0, 'a', {size: 2})", "text: size must be a number from 4 to 400"),
        ("arrow([0, 0])", "arrow: takes (x1, y1, x2, y2, opts?) or (from, to, opts?)"),
        ("arrow([0, 0], 'b')", "arrow: to must be [x, y] or a frame"),
        ("part('A b', () => 1)", "part: a part name is 1 to 40 of a-z, 0-9 and -"),
        ("part('a', () => part('b', () => 1))", "part: parts do not nest: b is inside a"),
        ("group({}, 3)", "group: the last argument must be a function"),
        ("plot(0, 0, 10, 10, {x: [0, 1]})", "plot: y must be [min, max]"),
        ("table(0, 0, [['a']]).cell(1, 0)", "cell: (1, 0) is not a cell"),
        ("box(0, 0, 1, 1, {dash: 'wavy'})", "box: dash must be"),
        ("undefinedThing()", "ReferenceError: undefinedThing is not defined"),
    ] {
        let e = failed(script);
        assert!(e.message.starts_with(said), "{script}: {e}");
        assert_eq!(e.line, 1, "{script}: {e}");
    }
    let e = failed("function helper() {\n  return box(0, 0, -1, 1);\n}\nhelper();");
    assert_eq!((e.line, e.col), (2, 10), "the line of the call, not of the caller: {e}");
    let e = failed("let x = ;");
    assert!(e.message.starts_with("SyntaxError"), "{e}");
    assert_eq!(e.line, 1);
}

#[test]
fn an_arrow_takes_lines_arguments_or_two_ends_and_meets_a_frame_at_its_edge() {
    let d = drawn(
        "const a = box(0, 0, 100, 50);\nconst b = box(300, 0, 100, 50);\narrow(a, b);\narrow([0, 100], b);",
    );
    assert_eq!(
        d.items[2]["pts"],
        json!([1000, 250, 2000, 0]),
        "from a's right edge to b's left edge"
    );
    assert_eq!(d.items[3]["pts"][0], 0);
    assert_eq!(d.items[3]["pts"][1], 1000);
}

#[test]
fn an_arrows_label_sits_beside_it_not_on_it() {
    let d = drawn(
        "arrow(0, 100, 200, 100, {label: 'goes'});\narrow(400, 0, 400, 200, {label: 'down'});",
    );
    let label =
        |i: usize| (d.items[i]["x"].as_i64(), d.items[i]["y"].as_i64(), d.items[i]["h"].as_i64());
    let (_, y, h) = label(1);
    assert!(y.zip(h).is_some_and(|(y, h)| y + h <= 1000), "above the line: {:?}", d.items[1]);
    let (x, _, _) = label(3);
    assert!(x.is_some_and(|x| x > 4000), "right of the line: {:?}", d.items[3]);
    assert!(d.warnings.is_empty(), "{:?}", d.warnings);
}

#[test]
fn labels_of_two_arrows_into_one_box_do_not_cover_each_other() {
    // The fluency round's state diagram: "cancel" and "refund" met at the middle.
    let d = drawn(
        "const a = box(0, 0, 150, 56);
const b = box(240, 0, 150, 56);
const c = box(120, 180, 150, 56);
         arrow(a, c, {label: 'cancel'});
arrow(b, c, {label: 'refund'});",
    );
    assert!(d.warnings.is_empty(), "{:?}", d.warnings);
    let texts: Vec<&Value> = d.items.iter().filter(|i| kind(i) == "text").collect();
    assert_eq!(texts.len(), 2);
}

#[test]
fn a_plots_ticks_take_labels_and_a_title_stands_above_it() {
    let d = drawn(
        "plot(0, 100, 200, 100, {x: [0, 2], y: [0, 10], xticks: [[0, 'Jan'], [1, 'Feb'], 2],
           ylabel: 'sales', title: 'Monthly'});",
    );
    let texts: Vec<&str> = d.items.iter().filter_map(|i| i["text"].as_str()).collect();
    assert_eq!(texts, ["Jan", "Feb", "2", "sales", "Monthly"]);
    let title = d.items.iter().find(|i| i["text"] == "Monthly").unwrap();
    // Half as large again as the labels' 16: 24, the default, which is not stored.
    assert_eq!((title.get("size"), &title["y"]), (None, &json!(1000 - 40 - 200 - 240)));
    assert!(d.warnings.is_empty(), "{:?}", d.warnings);
    let e = failed("plot(0, 0, 10, 10, {x: [0, 1], y: [0, 1], xticks: ['Jan']})");
    assert!(e.message.contains(r#"[value, "label"] pairs, such as [[0, "Jan"]"#), "{e}");
}

#[test]
fn warnings_name_text_over_a_line_overlapping_texts_and_a_far_shape() {
    let d = drawn(
        "line(0, 10, 100, 10);\ntext(0, 0, 'crossed');\ntext(5, 5, 'again');\nbox(0, 0, 1, 1);\nbox(90000, 0, 1, 1);",
    );
    let said: Vec<String> = d.warnings.iter().map(ToString::to_string).collect();
    assert_eq!(
        said,
        [
            r#"script:2:1: text: "crossed" is over the line from line 1"#,
            r#"script:3:1: text: "again" is over the line from line 1"#,
            r#"script:3:1: text: "again" overlaps the text from line 2"#,
            "script:5:1: box: is more than 5000 units from the other shapes",
        ]
    );
    assert_eq!(d.items.len(), 5, "a warning never fails the draw");
}

#[test]
fn stacks_and_tables_place_one_thing_after_another() {
    let d = drawn(
        "const f = stack(10, 20, {gap: 4}, (s) => { s.text('one'); s.gap(10); s.box(30, 6); });
         const t = table(0, 100, [['a', 'bb'], ['c']], {cols: [40, 60], parts: [[null, 'x'], ['y']]});
         warn(JSON.stringify([f, t.cell(0, 1)]));",
    );
    assert_eq!(
        d.items[1]["y"],
        (20 + 24 + 4 + 10) * 10,
        "after the text, the gap and the extra gap"
    );
    assert_eq!(
        d.warnings[0].message,
        r#"[{"x":10,"y":20,"w":36,"h":44},{"x":40,"y":100,"w":60,"h":38.400000000000006}]"#
    );
    let parts: Vec<Option<&str>> = d.items[2..].iter().map(|i| i["part"].as_str()).collect();
    assert_eq!(parts, [None, None, Some("x"), Some("x"), Some("y"), Some("y")]);
}

#[test]
fn a_plot_maps_values_and_takes_the_series_in_order() {
    let d = drawn(
        "const p = plot(0, 0, 100, 100, {x: [0, 10], y: [0, 1]});
         warn(JSON.stringify(p.at(5, 0.5)));
         p.line([[0, 0], [10, 1]]);
         p.dots([[5, 0.5]]);
         p.line((x) => x / 10, {color: 'ink', steps: 2});",
    );
    assert_eq!(d.warnings[0].message, "[50,50]");
    let strokes: Vec<&Value> = d.items.iter().filter(|i| kind(i) == "stroke").collect();
    let colours: Vec<&str> = strokes.iter().filter_map(|i| i["color"].as_str()).collect();
    assert_eq!(colours, ["#2f80ed", "#d55e00", "#000000"]);
    assert_eq!(strokes[1]["fill"], "#d55e0066", "a dot is filled");
    assert_eq!(strokes[2]["pts"].as_array().map(Vec::len), Some(6));
}

#[test]
fn the_script_sees_its_name_data_and_view() {
    let v = View { target: Some([500.0, 600.0, 300.0, 200.0]), ..view() };
    let d = run(
        "warn(JSON.stringify([fig.name, fig.data, fig.view, fig.color]));",
        &json!({"n": 1}),
        &v,
    );
    let said = d.map(|d| d.warnings[0].message.clone()).unwrap_or_default();
    assert_eq!(
        said,
        r##"["fig",{"n":1},{"target":{"x":0,"y":0,"w":300,"h":200},"page":"p1"},"#8a5cf5"]"##
    );
}
