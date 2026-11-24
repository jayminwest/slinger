//! The mirror reads an item's style as the core reads a file, and writes
//! it back as the core writes it.

use serde_json::json;

use super::super::{item, object};
use crate::config::Rgb;
use crate::scene::canvas::{Dash, Heads, Style};

#[test]
fn an_item_reads_and_writes_its_style_back_the_same() {
    let objects = [
        json!({"id": "i1", "kind": "box", "color": "#000000", "x": 0, "y": 0, "w": 100, "h": 50,
            "fill": "#ff000033", "dash": "dashed"}),
        json!({"id": "i2", "kind": "stroke", "color": "#8a5cf5", "width": 20,
            "pts": [0, 0, 100, 0, 0, 100], "fill": "#00ff00", "dash": "dotted", "closed": true}),
        json!({"id": "i3", "kind": "arrow", "color": "#000000", "pts": [0, 0, 100, 0],
            "heads": "both", "dash": "dashed"}),
        json!({"id": "i4", "kind": "arrow", "color": "#000000", "pts": [0, 0, 100, 0],
            "heads": "none"}),
    ];
    let styles: Vec<Style> = objects.iter().map(|o| item(o).unwrap().style).collect();
    assert_eq!(
        styles[0],
        Style {
            fill: Some((Rgb { r: 255, g: 0, b: 0 }, 0x33)),
            dash: Dash::Dashed,
            ..Style::default()
        }
    );
    assert_eq!(
        styles[1],
        Style {
            fill: Some((Rgb { r: 0, g: 255, b: 0 }, 255)),
            dash: Dash::Dotted,
            closed: true,
            ..Style::default()
        }
    );
    assert_eq!(styles[2], Style { heads: Heads::Both, dash: Dash::Dashed, ..Style::default() });
    assert_eq!(styles[3], Style { heads: Heads::None, ..Style::default() });
    for o in &objects {
        let written = object(&item(o).unwrap()).unwrap();
        for key in ["fill", "dash", "heads", "closed"] {
            assert_eq!(written.get(key), o.get(key), "{key} of {o}");
        }
    }
}

#[test]
fn a_field_the_kind_does_not_take_or_does_not_read_is_the_default() {
    let styled = |o: serde_json::Value| item(&o).unwrap().style;
    let plain = Style::default();
    // A fill on an open stroke, heads on a box, closed on a box.
    assert_eq!(
        styled(json!({"id": "i1", "kind": "stroke", "width": 20, "pts": [0, 0, 10, 0],
            "fill": "#ff0000"})),
        plain
    );
    assert_eq!(
        styled(json!({"id": "i1", "kind": "box", "x": 0, "y": 0, "w": 10, "h": 10,
            "heads": "both", "closed": true})),
        plain
    );
    // A fill that is not a colour is no fill, never black; an unknown word is solid.
    assert_eq!(
        styled(json!({"id": "i1", "kind": "box", "x": 0, "y": 0, "w": 10, "h": 10,
            "fill": "agent", "dash": "wavy"})),
        plain
    );
    // A text takes none.
    assert_eq!(
        styled(json!({"id": "i1", "kind": "text", "x": 0, "y": 0, "w": 10, "h": 10, "text": "a",
            "dash": "dashed"})),
        plain
    );
}
