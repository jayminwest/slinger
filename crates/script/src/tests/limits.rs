//! A script that loops forever, eats memory or recurses without end stops
//! within its limit with an error that names the line (plan acceptance 4).

use std::ops::RangeInclusive;
use std::time::{Duration, Instant};

use serde_json::json;

use super::{draw, failed, view};
use crate::{DATA_BYTES, ITEMS, SCRIPT_BYTES, TIME, run};

/// Each limit stops a run well inside this, the time limit with room for a loaded machine.
const SOON: Duration = Duration::from_secs(5);

fn stops(script: &str, said: &str, lines: RangeInclusive<u32>) {
    let start = Instant::now();
    let e = failed(script);
    assert!(start.elapsed() < SOON, "{script} took {:?}", start.elapsed());
    assert!(e.message.contains(said), "{e}");
    assert!(lines.contains(&e.line), "{e}");
    assert!(e.line == 0 || e.to_string().starts_with(&format!("script:{}:", e.line)), "{e}");
}

#[test]
fn an_endless_loop_stops_after_the_time_limit_at_its_line() {
    let start = Instant::now();
    stops(
        "box(0, 0, 10, 10);\nconst seen = [1, 2];\nfor (;;) {\n  seen.reverse();\n}\n",
        "ran longer than 1 s",
        4..=4,
    );
    assert!(start.elapsed() >= TIME);
}

#[test]
fn a_loop_that_catches_the_interrupt_still_stops() {
    let start = Instant::now();
    let e = failed("for (;;) {\n  try { for (;;) {} } catch (e) {}\n}\n");
    assert!(start.elapsed() < SOON);
    assert!(e.message.contains("ran longer than 1 s"), "{e}");
}

#[test]
fn a_promise_chain_without_end_stops_too() {
    // Stopped as a job starts, before its function has a frame, it names no line.
    stops(
        "function again() {\n  Promise.resolve().then(again);\n}\nagain();\n",
        "ran longer than 1 s",
        0..=2,
    );
}

#[test]
fn a_memory_hog_stops_at_the_memory_limit_at_its_line() {
    stops(
        "const all = [];\nfor (;;) {\n  all.push(new Array(100000).fill(7));\n}\n",
        "more than 32 MB",
        3..=3,
    );
}

#[test]
fn long_strings_stop_at_the_memory_limit() {
    stops(
        "const all = [];\nfor (;;) {\n  all.push('x'.repeat(100000) + all.length);\n}\n",
        "more than 32 MB",
        3..=3,
    );
}

#[test]
fn endless_recursion_stops_at_the_stack_limit_at_its_line() {
    stops(
        "function down(n) {\n  return down(n + 1) + 1;\n}\ndown(0);\n",
        "recursed too deep",
        2..=2,
    );
}

#[test]
fn too_many_shapes_fail_at_the_call() {
    let e = failed("for (let i = 0; i < 6000; i += 1) {\n  box(i, 0, 1, 1);\n}\n");
    assert_eq!(e.line, 2);
    assert!(e.message.contains(&format!("at most {ITEMS} shapes")), "{e}");
}

#[test]
fn a_script_or_data_too_large_fails_before_it_runs() {
    let long = format!("// {}\n", "x".repeat(SCRIPT_BYTES));
    let e = failed(&long);
    assert_eq!((e.line, e.col), (0, 0));
    assert!(e.to_string().starts_with("script: the script is"), "{e}");
    let data = json!({"big": "y".repeat(DATA_BYTES)});
    let e = run("box(0, 0, 1, 1);", &data, &view()).err().map(|e| e.message).unwrap_or_default();
    assert!(e.contains("the data is"), "{e}");
}

#[test]
fn the_script_cannot_reach_a_clock_a_timer_the_console_or_eval() {
    for (script, said) in [
        ("Date.now();", "Date is not defined"),
        ("setTimeout(() => 1, 0);", "setTimeout is not defined"),
        ("console.log(1);", "console is not defined"),
        ("eval('1');", "eval is not defined"),
        ("Function('return 1')();", "Function is not defined"),
        ("(function () {}).constructor('return 1')();", "not a function"),
        ("require('fs');", "require is not defined"),
        ("fetch('x');", "fetch is not defined"),
    ] {
        let e = failed(script);
        assert!(e.message.contains(said), "{script}: {e}");
        assert_eq!(e.line, 1, "{script}: {e}");
    }
    assert_eq!(draw("import('x');").map(|d| d.items.len()), Ok(0), "an import loads nothing");
    let late = draw("async function later() {\n  await 1;\n  box(0, 0, 1, 1);\n}\nlater();\n");
    assert_eq!(late.map(|d| d.items.len()), Ok(1), "promise jobs run before the run ends");
}

#[test]
fn math_random_is_seeded_so_runs_repeat() {
    let script = "for (let i = 0; i < 5; i += 1) box(Math.random() * 100, 0, 1, 1);";
    let a = draw(script).map(|d| d.items).ok();
    assert_eq!(a, draw(script).map(|d| d.items).ok());
    let xs: Vec<i64> = a.unwrap_or_default().iter().filter_map(|i| i["x"].as_i64()).collect();
    assert_eq!(xs.len(), 5);
    assert!(xs.windows(2).any(|w| w[0] != w[1]), "{xs:?}");
}
