//! Tests: the reference worksheet, the API's errors and warnings, the limits.

use serde_json::{Value, json};

use crate::{AGENT, Drawn, ScriptError, View, run};

mod api;
mod limits;
mod report;
mod worksheet;

fn view() -> View {
    View { name: "fig".into(), agent: AGENT.into(), page: "p1".into(), ..View::default() }
}

fn draw(script: &str) -> Result<Drawn, ScriptError> {
    run(script, &json!({}), &view())
}

fn drawn(script: &str) -> Drawn {
    match draw(script) {
        Ok(d) => d,
        Err(e) => unreachable!("{script}: {e}"),
    }
}

fn failed(script: &str) -> ScriptError {
    match draw(script) {
        Ok(d) => unreachable!("{script} drew {:?}", d.items),
        Err(e) => e,
    }
}

fn kind(item: &Value) -> &str {
    item["kind"].as_str().unwrap_or_default()
}
