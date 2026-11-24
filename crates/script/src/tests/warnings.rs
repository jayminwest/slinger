//! Warnings come back beside the shapes and never fail a draw: the
//! owner's three (a label taller than its box, a word wider than its
//! frame, two parts that overlap) and the ones the fluency round added.

use serde_json::Value;

use super::{drawn, kind};

fn said(script: &str) -> Vec<String> {
    drawn(script).warnings.iter().map(ToString::to_string).collect()
}

#[test]
fn a_label_taller_than_its_box_warns() {
    assert_eq!(
        said(
            "fig.box(0, 0, 100, 30, {label: 'a label of many words'});\nfig.box(0, 100, 300, 60, {label: 'fits'});"
        ),
        [
            r#"script:1:4: fig.box: the label "a label of many words" is 60 tall in a box 30 tall; make the box taller or the label shorter"#
        ]
    );
}

#[test]
fn a_word_wider_than_its_frame_warns() {
    assert_eq!(
        said(
            "fig.text('a supercalifragilistic word', 0, 0, {w: 100});\nfig.text('short words only', 0, 200, {w: 100});"
        ),
        [
            r#"script:1:4: fig.text: the word "supercalifragilistic" is 240 wide, wider than the text (100); it is cut"#
        ]
    );
    let label = said("fig.box(0, 0, 100, 200, {label: 'Antidisestablishment'});");
    assert_eq!(
        label,
        [
            r#"script:1:4: fig.box: the word "Antidisestablishment" is 200 wide, wider than the label of the box (80); it is cut"#
        ]
    );
}

#[test]
fn a_plots_first_y_label_sits_above_its_x_axis_clear_of_the_first_x_label() {
    let script = "fig.plot(40, 0, 300, 200, {xRange: [-1, 5], yRange: [-5, 30], ticks: 5, frame: 'axes'});";
    assert_eq!(said(script), [] as [String; 0]);
}

#[test]
fn two_parts_that_overlap_warn() {
    assert_eq!(
        said(
            "fig.box(0, 0, 100, 100, {name: 'a'});\nfig.box(50, 50, 100, 100, {name: 'b'});\nfig.box(150, 0, 10, 10, {name: 'c'});"
        ),
        [concat!(
            r#"script:2:5: name: the part "b" overlaps the part "a" from line 1: ink where "#,
            "their frames meet may be said to be over either"
        )],
        "touching is not overlapping"
    );
}

#[test]
fn warnings_name_text_over_a_line_overlapping_texts_and_a_far_shape() {
    let d = drawn(
        "fig.line(0, 10, 100, 10);\nfig.text('crossed', 0, 0);\nfig.text('again', 5, 5);\nfig.box(0, 0, 1, 1);\nfig.box(90000, 0, 1, 1);",
    );
    let said: Vec<String> = d.warnings.iter().map(ToString::to_string).collect();
    assert_eq!(
        said,
        [
            r#"script:2:5: fig.text: "crossed" is over the line from line 1"#,
            r#"script:3:5: fig.text: "again" is over the line from line 1"#,
            r#"script:3:5: fig.text: "again" overlaps the text "crossed" from line 2"#,
            "script:5:5: fig.box: is more than 5000 units from the other shapes",
        ]
    );
    assert_eq!(d.items.len(), 5, "a warning never fails the draw");
}

#[test]
fn an_arrows_label_sits_beside_it_not_on_it() {
    let d = drawn(
        "fig.arrow(0, 100, 200, 100, {label: 'goes'});\nfig.arrow(400, 0, 400, 200, {label: 'down'});",
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
        "const a = fig.box(0, 0, 150, 56);
const b = fig.box(240, 0, 150, 56);
const c = fig.box(120, 180, 150, 56);
         fig.arrow(a, c, {label: 'cancel'});
fig.arrow(b, c, {label: 'refund'});",
    );
    assert!(d.warnings.is_empty(), "{:?}", d.warnings);
    let texts: Vec<&Value> = d.items.iter().filter(|i| kind(i) == "text").collect();
    assert_eq!(texts.len(), 2);
}

#[test]
fn a_box_drawn_again_on_a_tables_cell_warns_and_a_filled_one_does_not() {
    // The fluency round's vector addition: each empty answer cell boxed twice.
    let d = drawn(
        "const t = fig.table([['a', ''], ['b', '']], 0, 0, {widths: 60, header: false, parts: [[null, 'x'], [null, 'y']]});
const c = t.cell(0, 1); fig.box(c.x, c.y, c.w, c.h, {name: 'x'});
const e = t.cell(1, 1);
fig.box(e.x, e.y, e.w, e.h, {fill: 'yellow'});",
    );
    let said: Vec<String> = d.warnings.iter().map(ToString::to_string).collect();
    assert_eq!(
        said,
        ["script:2:48: fig.box: is drawn again on the box from line 1; \
          a table's cell is boxed already, draw in it with t.cell(r, c)"]
    );
}

#[test]
fn a_label_longer_than_its_arrow_warns() {
    let d = drawn(
        "fig.arrow(0, 0, 40, 0, {label: 'much too long'});\nfig.arrow(0, 100, 0, 200, {label: 'fits'});",
    );
    let said: Vec<String> = d.warnings.iter().map(ToString::to_string).collect();
    assert_eq!(
        said,
        [
            r#"script:1:4: fig.arrow: the label "much too long" is 156 long beside an arrow 40 long; lengthen the arrow or shorten the label"#
        ]
    );
    assert_eq!(d.items.len(), 4, "a warning never fails the draw");
}
