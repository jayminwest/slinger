use super::pack::decode;
use super::*;

/// The requests in `bytes`, as (method, args).
fn sent(bytes: &[u8]) -> Vec<(String, Vec<Value>)> {
    let (mut at, mut out) = (0, Vec::new());
    while at < bytes.len() {
        let (value, took) = decode(&bytes[at..]).unwrap();
        at += took;
        if let Some([_, _, method, Value::Array(args)]) = value.as_array() {
            out.push((method.as_str().unwrap().to_string(), args.clone()));
        }
    }
    out
}

fn methods(bytes: &[u8]) -> Vec<String> {
    sent(bytes).into_iter().map(|(m, _)| m).collect()
}

fn notify(method: &str, params: Vec<Value>) -> Value {
    Value::Array(vec![Value::Int(2), Value::str(method), Value::Array(params)])
}

fn lines(tick: Value, first: i64, last: i64, lines: &[&str]) -> Value {
    let lines = Value::Array(lines.iter().map(|l| Value::str(l)).collect());
    let params =
        vec![Value::Int(1), tick, Value::Int(first), Value::Int(last), lines, Value::Bool(false)];
    notify("nvim_buf_lines_event", params)
}

#[test]
fn the_buffer_is_attached_only_once_ready_and_what_came_before_follows_in_order() {
    let mut out = Vec::new();
    let mut session = Session::start("plan", false, &mut out);
    assert_eq!(methods(&out), ["nvim_ui_attach", "nvim_exec_lua"]);
    out.clear();
    session.input("ihello", &mut out);
    session.paste("world", &mut out);
    session.barrier(&mut out);
    assert!(out.is_empty(), "nothing goes before the config has loaded");
    assert!(!session.ready());
    assert_eq!(session.hear(&notify("slinger_ready", vec![]), &mut out), Heard::Ready);
    assert_eq!(methods(&out), ["nvim_buf_attach", "nvim_input", "nvim_paste", "nvim_eval"]);
    let keys = &sent(&out)[1].1;
    assert_eq!(keys, &[Value::str("ihello")]);
    out.clear();
    assert_eq!(session.hear(&notify("slinger_ready", vec![]), &mut out), Heard::Nothing);
    session.input("x", &mut out);
    assert_eq!(methods(&out), ["nvim_input"], "once ready, a request goes at once");
}

#[test]
fn the_text_is_confirmed_only_by_the_whole_buffer() {
    let mut out = Vec::new();
    let mut session = Session::start("one\ntwo", false, &mut out);
    assert_eq!(session.text(), None);
    assert_eq!(session.hear(&lines(Value::Int(2), 1, 2, &["x"]), &mut out), Heard::Nothing);
    assert_eq!(session.text(), None, "a part of a buffer confirms nothing");
    assert_eq!(session.hear(&lines(Value::Int(2), 0, -1, &[""]), &mut out), Heard::Lines);
    assert_eq!(session.text().as_deref(), Some(""), "an empty buffer is a confirmed text");
}

#[test]
fn line_events_keep_the_mirror_and_a_preview_does_not() {
    let mut out = Vec::new();
    let mut session = Session::start("one\ntwo\nthree", false, &mut out);
    let whole = lines(Value::Int(2), 0, -1, &["one", "two", "three"]);
    assert_eq!(session.hear(&whole, &mut out), Heard::Lines);
    assert_eq!(session.hear(&lines(Value::Int(3), 1, 2, &["2", "2.5"]), &mut out), Heard::Lines);
    assert_eq!(session.text().as_deref(), Some("one\n2\n2.5\nthree"));
    assert_eq!(session.hear(&lines(Value::Nil, 0, 1, &["x"]), &mut out), Heard::Nothing);
    assert_eq!(session.hear(&lines(Value::Int(4), 0, 4, &[]), &mut out), Heard::Lines);
    assert_eq!(session.text().as_deref(), Some(""));
}

#[test]
fn replies_mean_a_barrier_a_broken_seed_or_a_failure_and_requests_get_an_error() {
    let mut out = Vec::new();
    let mut session = Session::start("", true, &mut out);
    let reply = |id: i64, error: Value| {
        Value::Array(vec![Value::Int(1), Value::Int(id), error, Value::Nil])
    };
    let error = Value::Array(vec![Value::Int(0), Value::str("E5108: boom")]);
    assert_eq!(
        session.hear(&reply(2, error.clone()), &mut out),
        Heard::Broken("E5108: boom".into())
    );
    assert_eq!(session.hear(&reply(1, error), &mut out), Heard::Failed("E5108: boom".into()));
    session.hear(&notify("slinger_ready", vec![]), &mut out);
    out.clear();
    session.barrier(&mut out);
    let id = 4; // 1 attach, 2 seed, 3 buffer attach
    assert_eq!(session.hear(&reply(id, Value::Nil), &mut out), Heard::Barrier);
    out.clear();
    let asked =
        Value::Array(vec![Value::Int(0), Value::Int(9), Value::str("x"), Value::Array(vec![])]);
    assert_eq!(session.hear(&asked, &mut out), Heard::Nothing);
    assert_eq!(decode(&out).unwrap().0.as_array().unwrap()[2], Value::str("not served"));
}

#[test]
fn the_mode_follows_mode_change_and_a_flush_is_heard() {
    let mut out = Vec::new();
    let mut session = Session::start("", false, &mut out);
    let change = |mode: &str| {
        Value::Array(vec![
            Value::str("mode_change"),
            Value::Array(vec![Value::str(mode), Value::Int(1)]),
        ])
    };
    let flush = Value::Array(vec![Value::str("flush"), Value::Array(vec![])]);
    assert_eq!(session.hear(&notify("redraw", vec![change("insert")]), &mut out), Heard::Nothing);
    assert!(!session.normal());
    assert_eq!(
        session.hear(&notify("redraw", vec![change("normal"), flush]), &mut out),
        Heard::Flushed
    );
    assert!(session.normal());
    assert_eq!(
        session.hear(&notify("slinger_old", vec![Value::str("0.9.5")]), &mut out),
        Heard::Old("0.9.5".into())
    );
}

#[test]
fn each_flush_once_ready_measures_and_the_grid_follows_the_text_and_the_gutter() {
    let mut out = Vec::new();
    let mut session = Session::start("one", false, &mut out);
    let flush =
        || notify("redraw", vec![Value::Array(vec![Value::str("flush"), Value::Array(vec![])])]);
    out.clear();
    assert_eq!(session.hear(&flush(), &mut out), Heard::Flushed);
    assert!(out.is_empty(), "no measure before ready: the window is not set up");
    session.hear(&notify("slinger_ready", vec![]), &mut out);
    out.clear();
    session.hear(&flush(), &mut out);
    assert_eq!(methods(&out), ["nvim_exec_lua"], "a flush asks how tall the text is");
    let measure = 4; // 1 attach, 2 seed, 3 buffer attach
    out.clear();
    session.hear(&flush(), &mut out);
    assert!(out.is_empty(), "one measure out at a time");
    let answer = |id: i64, rows: i64, gutter: i64| {
        let result = Value::Array(vec![Value::Int(rows), Value::Int(gutter)]);
        Value::Array(vec![Value::Int(1), Value::Int(id), Value::Nil, result])
    };
    assert_eq!(session.hear(&answer(measure, 3, 2), &mut out), Heard::Nothing);
    let asked = sent(&out);
    let (resize, again) = (&asked[0], &asked[1]);
    assert_eq!(resize.0, "nvim_ui_try_resize");
    assert_eq!(
        resize.1,
        [Value::Int(82), Value::Int(4)],
        "80 text cells, the gutter, and the : line"
    );
    assert_eq!(again.0, "nvim_exec_lua", "the flush that came meanwhile is measured too");
    let resized = Value::Array(vec![
        Value::str("grid_resize"),
        Value::Array(vec![Value::Int(1), Value::Int(82), Value::Int(4)]),
    ]);
    session.hear(&notify("redraw", vec![resized]), &mut out);
    out.clear();
    session.hear(&answer(6, 3, 2), &mut out);
    assert!(out.is_empty(), "the grid is that size: no resize");
    session.hear(&flush(), &mut out);
    out.clear();
    session.hear(&answer(7, 500, 0), &mut out);
    assert_eq!(sent(&out)[0].1, [Value::Int(80), Value::Int(81)], "at most 80 rows of text");
}
