//! The examples agents read run as written: the owner's in
//! docs/design/figures.md (Appendix A) and the guide's "Drawing scripts".

use serde_json::{Value, json};

use super::view;
use crate::run;

const DESIGN: &str = include_str!("../../../../docs/design/figures.md");
const GUIDE: &str = include_str!("../../../cli/src/guide.md");

/// The first js code block after `heading` in `doc`.
fn example(doc: &str, heading: &str) -> String {
    let after = &doc[doc.find(heading).unwrap_or_default()..];
    let at =
        after.find("#### Example").or_else(|| after.find("**An example.**")).unwrap_or_default();
    let code = &after[at..];
    let start = code.find("```js\n").map(|i| i + 6).unwrap_or_default();
    code[start..start + code[start..].find("```").unwrap_or_default()].to_owned()
}

fn clean(script: &str, data: &Value) -> usize {
    match run(script, data, &view()) {
        Ok(d) => {
            assert!(d.warnings.is_empty(), "{script}: {:?}", d.warnings);
            d.items.len()
        }
        Err(e) => unreachable!("{script}: {e}"),
    }
}

#[test]
fn the_owners_example_runs() {
    let script = example(DESIGN, "## Appendix A");
    assert!(script.contains("p.bars"), "{script}");
    // A title, a frame, three bars and their labels, a slot and its label.
    assert_eq!(clean(&script, &Value::Null), 1 + 1 + 3 + 3 + 2);
}

#[test]
fn the_guides_example_runs_with_its_data() {
    let script = example(GUIDE, "## Drawing scripts");
    let data = json!({"title": "Rain (mm)", "rows": [["Jan", 80], ["Feb", 65], ["Mar", 90]]});
    // A title, a frame, two ticks, three bars and their labels, a slot and its label.
    assert_eq!(clean(&script, &data), 1 + 1 + 2 + 3 + 3 + 2);
}
