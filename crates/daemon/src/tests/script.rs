//! The script engine against the core it feeds: a script's shapes are
//! `add` item objects the core's apply reads, and a script measures text by
//! the core's rule (docs/design/figures.md, "How shapes cross to the daemon").

use serde_json::{Value, json};
use slinger_core::canvas::file::Log;
use slinger_core::canvas::style::{self, Colour, Dash, Heads, Style};
use slinger_core::canvas::{Author, PageId, Time};
use slinger_core::render::text_frame;
use slinger_script::{AGENT, COLOURS, Drawn, View, run};

const AT: Time = Time(1_762_790_400_000);
const WORKSHEET: &str = include_str!("../../../script/src/tests/worksheet.js");
const DATA: &str = include_str!("../../../script/src/tests/worksheet.json");

fn drawn(script: &str, data: &Value) -> Drawn {
    let view = View {
        name: "worksheet".into(),
        agent: AGENT.into(),
        page: "p1".into(),
        ..View::default()
    };
    match run(script, data, &view) {
        Ok(d) => d,
        Err(e) => unreachable!("{e}"),
    }
}

#[test]
fn a_scripts_shapes_are_items_the_core_applies_as_one_step() {
    let data: Value = serde_json::from_str(DATA).unwrap_or_default();
    let d = drawn(WORKSHEET, &data);
    let ops: Vec<Value> = d
        .items
        .into_iter()
        .map(|mut item| {
            item["page"] = json!("p1");
            json!({"op": "add", "item": item})
        })
        .collect();
    let (mut log, _) = Log::create("c").unwrap_or_else(|e| unreachable!("{e}"));
    let written = log.apply(&Author::Agent("claude".into()), AT, &Value::Array(ops));
    assert_eq!(written.map(|w| w.made.len()).map_err(|e| e.to_string()), Ok(56));
    let styles: Vec<Style> = log.canvas().items(PageId::new(1)).map(|i| *i.style()).collect();
    let count = |f: fn(&Style) -> bool| styles.iter().filter(|s| f(s)).count();
    assert_eq!(count(|s| s.heads == Heads::None), 3, "a line is an arrow with no heads");
    assert_eq!(count(|s| s.dash == Dash::Dashed), 4, "the four answer areas");
    assert_eq!(count(|s| s.dash == Dash::Dotted), 2, "the two areas the agent fills");
    assert_eq!(count(|s| s.fill.is_some()), 6, "the table's header row");
    assert_eq!(count(|s| s.closed), 1, "the check circle");
}

#[test]
fn the_scripts_agent_colour_is_the_cores() {
    assert_eq!(Colour::parse(AGENT), Some(style::AGENT));
    assert_eq!(COLOURS.iter().find(|(name, _)| *name == "agent").map(|(_, hex)| *hex), Some(AGENT));
}

#[test]
fn a_script_measures_text_as_the_core_does() {
    let cases = [
        ("hello world", 0.0, 24.0),
        ("hello world", 60.0, 24.0),
        ("a much longer line that wraps at its spaces", 100.0, 20.0),
        ("x\ny\nzzz", 0.0, 16.0),
        ("\u{200B}hi\u{FE0F}", 0.0, 24.0),
        ("supercalifragilistic", 50.0, 24.0),
        ("tiny", 5.0, 24.0),
        ("über café", 0.0, 30.0),
    ];
    for (text, w, size) in cases {
        let opts = if w > 0.0 { json!({"size": size, "w": w}) } else { json!({"size": size}) };
        let script = format!(
            "const m = measure({}, {opts});\nwarn(JSON.stringify([m.w, m.h]));",
            json!(text)
        );
        let said = drawn(&script, &json!({}))
            .warnings
            .first()
            .map(|w| w.message.clone())
            .unwrap_or_default();
        let measured: Vec<f64> = serde_json::from_str(&said).unwrap_or_default();
        let (cw, ch) = text_frame(text, w, 0.0, size);
        assert_eq!(measured, [cw, ch], "{text:?} at {size} in {w}");
    }
}
