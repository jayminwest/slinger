//! Handles (docs/design/figures.md, Appendix A, "Handles" and
//! "Helpers"): bounds, moves, groups, stack, grid, table and plot. Nothing
//! is placed until the script ends, so a move after a call still counts.

use serde_json::json;

use super::api::one;
use super::{drawn, kind};

/// What the script warns with `warn(JSON.stringify(...))`, first.
fn said(script: &str) -> String {
    drawn(script).warnings.first().map(|w| w.message.clone()).unwrap_or_default()
}

#[test]
fn a_handle_has_bounds_and_moves_until_the_script_ends() {
    assert_eq!(
        said(
            "const b = fig.box(10, 20, 30, 40);\nwarn(JSON.stringify([b.x, b.y, b.w, b.h, b.right, b.bottom, b.cx, b.cy]));"
        ),
        "[10,20,30,40,40,60,25,40]"
    );
    let moved = |m: &str| one(&format!("fig.box(10, 20, 30, 40).{m}"));
    assert_eq!(
        (moved("moveTo(0, 5)")["x"].clone(), moved("moveTo(0, 5)")["y"].clone()),
        (json!(0), json!(50))
    );
    assert_eq!(moved("moveBy(1, -1)")["x"], 110);
    let d = drawn(
        "const a = fig.box(100, 100, 50, 50);
         const b = fig.box(0, 0, 10, 10).below(a);
         const c = fig.box(0, 0, 10, 10).above(a, 5);
         const e = fig.box(0, 0, 10, 10).rightOf(a);
         const f = fig.box(0, 0, 10, 10).leftOf(a, 0);
         const g = fig.box(0, 0, 10, 10).centerIn(a);
         fig.box(0, 0, 10, 10).moveTo(1, 1).moveBy(1, 1);",
    );
    let at: Vec<(i64, i64)> = d
        .items
        .iter()
        .map(|i| (i["x"].as_i64().unwrap_or(-1), i["y"].as_i64().unwrap_or(-1)))
        .collect();
    assert_eq!(at[1].1, 1740, "below: 24 under its bottom, x as it was");
    assert_eq!(at[2].1, 850, "above: its bottom 5 over the top");
    assert_eq!(at[3].0, 1740, "right of: 24 past its right");
    assert_eq!(at[4].0, 900, "left of: its right on the left edge");
    assert_eq!(at[5], (1200, 1200), "centred");
    assert_eq!(at[6], (20, 20), "moves chain");
}

#[test]
fn a_box_moves_with_its_label_and_an_arrow_meets_a_handles_edge() {
    let d = drawn("fig.box(0, 0, 100, 50, {label: 'x'}).moveTo(200, 0);");
    assert_eq!((d.items[0]["x"].clone(), d.items[1]["x"].clone()), (json!(2000), json!(2100)));
    let d = drawn(
        "const a = fig.box(0, 0, 100, 50);\nconst b = fig.box(300, 0, 100, 50);\nfig.arrow(a, b);\nfig.arrow([0, 100], b);",
    );
    assert_eq!(
        d.items[2]["pts"],
        json!([1000, 250, 2000, 0]),
        "from a's right edge to b's left edge"
    );
    assert_eq!(
        (d.items[3]["pts"][0].clone(), d.items[3]["pts"][1].clone()),
        (json!(0), json!(1000))
    );
}

#[test]
fn a_group_is_one_handle_over_several() {
    let d = drawn(
        "const a = fig.box(0, 0, 10, 10);
         const b = fig.text('hi', 20, 0);
         const g = fig.group(a, [b], {color: 'red'});
         warn(JSON.stringify(g));
         g.moveTo(100, 100);
         fig.group(g, a).moveBy(0, 1);",
    );
    assert_eq!(d.warnings[0].message, r#"{"x":0,"y":0,"w":44,"h":24}"#);
    assert_eq!(
        (d.items[0]["x"].clone(), d.items[0]["y"].clone()),
        (json!(1000), json!(1010)),
        "moved once, not twice"
    );
    assert_eq!(
        (d.items[1]["x"].clone(), d.items[1]["color"].clone()),
        (json!(1200), json!("#ff0000"))
    );
}

#[test]
fn a_stack_places_each_after_the_one_before_and_a_grid_in_rows() {
    let d = drawn(
        "fig.stack([fig.text('one', 10, 0), fig.box(0, 0, 30, 6), fig.box(5, 0, 1, 1)], {gap: 4});
         fig.stack([fig.box(0, 100, 10, 10), fig.box(0, 0, 20, 20)], {dir: 'right'});",
    );
    let y: Vec<i64> = d.items.iter().filter_map(|i| i["y"].as_i64()).collect();
    let x: Vec<i64> = d.items.iter().filter_map(|i| i["x"].as_i64()).collect();
    assert_eq!(&y[..3], [0, 280, 380], "24 tall, 4 apart, then 6 tall");
    assert_eq!(&x[..3], [100, 0, 50], "each keeps its x");
    assert_eq!((x[4], y[4]), (340, 0), "right: 24 past, its y as it was");
    let g = drawn(
        "fig.grid([fig.box(0, 0, 10, 10), fig.box(0, 0, 30, 5), fig.box(0, 0, 5, 20)], {cols: 2, gap: 2, x: 100, y: 50});",
    );
    let at: Vec<(i64, i64)> = g
        .items
        .iter()
        .map(|i| (i["x"].as_i64().unwrap_or(-1), i["y"].as_i64().unwrap_or(-1)))
        .collect();
    assert_eq!(at, [(1000, 500), (1120, 500), (1000, 620)], "columns as wide as their widest");
}

#[test]
fn a_table_fits_its_columns_tints_its_header_and_gives_its_cells() {
    let d = drawn(
        "const t = fig.table([['a', 'bbbb'], ['c', '']], 0, 100, {parts: [null, [null, 'x']]});
         t.moveBy(10, 0);
         warn(JSON.stringify([t.cell(1, 1), t.w]));",
    );
    let boxes: Vec<&serde_json::Value> = d.items.iter().filter(|i| kind(i) == "box").collect();
    let texts: Vec<&serde_json::Value> = d.items.iter().filter(|i| kind(i) == "text").collect();
    assert_eq!((boxes.len(), texts.len()), (4, 3), "a box per cell, a text per non-empty cell");
    assert_eq!(
        boxes.iter().filter(|b| b["fill"] == "#8a5cf533").count(),
        2,
        "the header row is tinted"
    );
    assert_eq!(texts[0]["size"], 200, "size 20");
    assert_eq!(d.warnings[0].message, r#"[{"x":50,"y":132,"w":60,"h":32},100]"#);
    assert_eq!(boxes[3]["part"], "x");
    let plain = drawn("fig.table([['a']], 0, 0, {header: false, widths: [50], size: 10})");
    assert_eq!((plain.items[0].get("fill"), plain.items[0]["w"].clone()), (None, json!(500)));
}

#[test]
fn a_plot_maps_data_draws_bars_curves_and_vectors_and_has_bounds() {
    let d = drawn(
        "const p = fig.plot(0, 0, 300, 100, {yRange: [0, 1]});
         warn(JSON.stringify([p.x(0.5), p.y(0.25)]));
         p.bars([0.5, 1, 0], {labels: ['A', 'B', 'C']});
         p.fn((x) => x, {steps: 2});
         p.vector([0, 0], [1, 1], {label: 'v'});
         p.bounds.name('plot');",
    );
    assert_eq!(d.warnings[0].message, "[150,75]");
    assert_eq!(
        (kind(&d.items[0]), d.items[0]["dash"].clone()),
        ("box", json!("dotted")),
        "a dotted frame"
    );
    let bars: Vec<&serde_json::Value> = d.items[1..].iter().filter(|i| kind(i) == "box").collect();
    assert_eq!(bars.len(), 2, "a bar of 0 is no box");
    assert_eq!(
        (bars[0]["x"].clone(), bars[0]["y"].clone(), bars[0]["w"].clone(), bars[0]["h"].clone()),
        (json!(200), json!(500), json!(600), json!(500))
    );
    assert_eq!(
        (bars[0]["color"].clone(), bars[0]["fill"].clone()),
        (json!("#2f80ed"), json!("#2f80ed33"))
    );
    let labels: Vec<&str> = d.items.iter().filter_map(|i| i["text"].as_str()).collect();
    assert_eq!(labels, ["A", "B", "C", "v"]);
    let curve = d.items.iter().find(|i| kind(i) == "stroke").unwrap();
    assert_eq!(
        (curve["color"].clone(), curve["pts"].as_array().map(Vec::len)),
        (json!("#d55e00"), Some(6))
    );
    assert_eq!(d.items.iter().filter(|i| kind(i) == "arrow").count(), 1);
    assert!(d.items.iter().all(|i| i["part"] == "plot"), "the bounds name everything in it");
    let axes = drawn("fig.plot(0, 0, 10, 10, {frame: 'axes', ticks: 2})");
    let texts: Vec<&str> = axes.items.iter().filter_map(|i| i["text"].as_str()).collect();
    assert_eq!(texts, ["0", "0.5", "1", "0", "0.5", "1"]);
    assert_eq!(axes.items.iter().filter(|i| i["heads"] == "none").count(), 2);
    assert_eq!(drawn("fig.plot(0, 0, 10, 10, {frame: 'none'})").items.len(), 0);
}

#[test]
fn a_plot_moves_whole_and_maps_where_it_is_now() {
    let d = drawn(
        "const p = fig.plot(0, 0, 100, 100);
         fig.group(p.bounds).moveBy(50, 0);
         warn(JSON.stringify(p.x(0)));
         p.dots([[0, 0]]);",
    );
    assert_eq!(d.warnings[0].message, "50");
    assert_eq!(d.items[0]["x"], 500);
    assert_eq!(d.items[1]["fill"], "#2f80ed66", "a dot is filled");
}

#[test]
fn a_plots_ticks_take_labels_and_a_title_stands_above_it() {
    let d = drawn(
        "fig.plot(0, 100, 200, 100, {xRange: [0, 2], yRange: [0, 10], xticks: [[0, 'Jan'], [1, 'Feb'], 2],
           ylabel: 'sales', title: 'Monthly', size: 16});",
    );
    let texts: Vec<&str> = d.items.iter().filter_map(|i| i["text"].as_str()).collect();
    assert_eq!(texts, ["Jan", "Feb", "2", "sales", "Monthly"]);
    let title = d.items.iter().find(|i| i["text"] == "Monthly").unwrap();
    // Half as large again as the labels' 16: 24, the default, which is not stored.
    assert_eq!((title.get("size"), &title["y"]), (None, &json!(1000 - 40 - 200 - 240)));
    assert!(d.warnings.is_empty(), "{:?}", d.warnings);
}
