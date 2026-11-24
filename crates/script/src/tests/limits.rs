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

/// Endless loops after three lines (shapes, `seen`, `w`), each with the
/// last line its loop runs: a stop can be anywhere up to there.
const LOOPS: [(&str, u32); 6] = [
    ("for (;;) {\n  w += 1;\n}\n", 6),
    ("for (;;) {\n  seen.reverse();\n}\n", 6),
    ("while (w >= 0) {\n  w += measure('abc').w;\n}\n", 6),
    ("for (let i = 0; i >= 0; i++) {\n  w = i;\n}\n", 6),
    ("function spin() {\n  do {\n    w += 1;\n  } while (w > 0);\n}\nspin();\n", 7),
    (
        "for (let i = 0; ; i++) {\n  if (i > 5) {\n    while (true) { w += 1; }\n  }\n  seen.reverse();\n}\n",
        6,
    ),
];

#[test]
fn an_endless_loop_stops_after_the_time_limit_at_or_before_its_line() {
    // Where QuickJS checks the clock moves with the work run before the
    // loop (the shapes drawn, the prelude's own branches): at a call or at
    // a jump back to the loop's head, which names a line before the loop.
    // So a call-free loop, one calling a native or a prelude function, one
    // in a function and a nested one, each after an odd and an even number
    // of shapes, name a line at or before the loop and say so
    // (slinger-71c6cb).
    std::thread::scope(|threads| {
        for shapes in [0, 1, 2, 7] {
            for (body, last) in LOOPS {
                threads.spawn(move || {
                    let script = format!(
                        "for (let i = 0; i < {shapes}; i += 1) fig.box(i, 0, 1, 1);\nconst seen = [1, 2];\nlet w = 0;\n{body}"
                    );
                    let start = Instant::now();
                    let e = failed(&script);
                    let took = start.elapsed();
                    assert!(took >= TIME && took < SOON, "{script} took {took:?}");
                    assert!((1..=last).contains(&e.line), "{script}: {e}");
                    let said = format!(
                        "script:{}:{}: the script ran longer than 1 s, stopped at this line or in a loop after it",
                        e.line, e.col
                    );
                    assert_eq!(e.to_string(), said, "{script}");
                });
            }
        }
    });
}

#[test]
fn a_loop_on_the_second_line_is_named_by_the_line_before_it_and_says_so() {
    // QuickJS's jump back polls the clock without saving its place: the
    // stop names the instruction before the loop's head (slinger-a0f694).
    let e = failed("let w = 0;\nwhile (true) {\n  w += 1;\n}\n");
    assert_eq!(
        e.to_string(),
        "script:1:1: the script ran longer than 1 s, stopped at this line or in a loop after it"
    );
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
    // When the memory left cannot hold the error itself QuickJS throws
    // null and names no line; where 32 MB falls moves with the prelude's
    // size. 150000 leaves the error room with the prelude as it is.
    stops(
        "const all = [];\nfor (;;) {\n  all.push('x'.repeat(150000) + all.length);\n}\n",
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
    let e = failed("for (let i = 0; i < 6000; i += 1) {\n  fig.box(i, 0, 1, 1);\n}\n");
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
    let e =
        run("fig.box(0, 0, 1, 1);", &data, &view()).err().map(|e| e.message).unwrap_or_default();
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
    let import = failed("import('x');").message;
    assert!(import.contains("could not load module 'x'"), "an import loads nothing: {import}");
    let late = draw("async function later() {\n  await 1;\n  fig.box(0, 0, 1, 1);\n}\nlater();\n");
    assert_eq!(late.map(|d| d.items.len()), Ok(1), "promise jobs run before the run ends");
}

#[test]
fn a_rejection_nothing_catches_fails_the_draw_at_its_line() {
    let e = failed(
        "fig.box(0, 0, 1, 1);\nasync function later() {\n  await 1;\n  throw new Error('no data');\n}\nlater();\n",
    );
    assert_eq!((e.line, e.col), (4, 13), "{e}");
    assert!(e.message.contains("nothing caught it") && e.message.contains("no data"), "{e}");
    let e = failed("Promise.reject('gone');\n");
    assert!(e.message.ends_with("nothing caught it: gone"), "{e}");
    let caught = "Promise.reject(new Error('x')).catch(() => fig.box(0, 0, 1, 1));\n";
    assert_eq!(draw(caught).map(|d| d.items.len()), Ok(1), "a rejection caught later is fine");
}

#[test]
fn math_random_is_seeded_so_runs_repeat() {
    let script = "for (let i = 0; i < 5; i += 1) fig.box(Math.random() * 100, 0, 1, 1);";
    let a = draw(script).map(|d| d.items).ok();
    assert_eq!(a, draw(script).map(|d| d.items).ok());
    let xs: Vec<i64> = a.unwrap_or_default().iter().filter_map(|i| i["x"].as_i64()).collect();
    assert_eq!(xs.len(), 5);
    assert!(xs.windows(2).any(|w| w[0] != w[1]), "{xs:?}");
}
