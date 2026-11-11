//! `slinger feedback ...` against a real daemon.

use std::path::Path;

use super::{Run, cli, cli_each, cli_on, json, with_daemon};
use crate::output::exit;

#[test]
fn add_and_path_print_the_ts_and_the_file() {
    let runs = cli_each(&[
        &["feedback", "path"],
        &["feedback", "ls"],
        &["feedback", "add", "the", "lasso", "is", "slow"],
        &["feedback", "add", "second", "--json"],
        &["feedback", "ls", "--limit", "1"],
    ]);
    let [path, none, added, doc, one] = &runs[..] else { panic!("five runs") };
    assert_eq!(path.code, exit::OK);
    assert!(path.out.ends_with("/feedback.jsonl\n"), "{}", path.out);
    assert_eq!(none.out, "", "none yet");
    assert_eq!((added.code, added.err.as_str()), (exit::OK, ""));
    let ts = added.out.trim_end();
    assert!(ts.len() == 25 && !ts.contains('\t'), "the ts alone: {ts}");
    let doc = json(doc);
    let (command, text) = (doc["command"].as_str(), doc["entry"]["text"].as_str());
    assert_eq!((command, text), (Some("feedback add"), Some("second")));
    assert_eq!(doc["path"].as_str().map(|p| format!("{p}\n")), Some(path.out.clone()));
    assert!(one.out.ends_with("\tsecond\n") && one.out.lines().count() == 1, "{}", one.out);
}

#[test]
fn resolve_hides_a_note_until_ls_all_and_twice_is_a_conflict() {
    with_daemon(|socket| {
        let run = |args: &[&str]| cli_on(socket, args);
        let added = run(&["feedback", "add", "the lasso is slow"]);
        let ts = added.out.trim_end();
        let done = run(&["feedback", "resolve", ts, "slinger-1234"]);
        assert_eq!((done.code, done.out.as_str(), done.err.as_str()), (exit::OK, "", ""));
        assert_eq!(run(&["feedback", "ls"]).out, "");
        let all = run(&["feedback", "ls", "--all"]).out;
        assert_eq!(all, format!("{ts}\tthe lasso is slow\tresolved slinger-1234\n"));
        let listed = json(&run(&["feedback", "ls", "--all", "--json"]));
        assert_eq!(listed["entries"][0]["resolved"]["note"], "slinger-1234");
        assert_eq!(run(&["feedback", "resolve", ts]).code, exit::CONFLICT);
        assert_eq!(run(&["feedback", "resolve", "never"]).code, exit::NOT_FOUND);
        assert!(Path::new(listed["path"].as_str().unwrap_or_default()).is_file());
    });
}

#[test]
fn blank_or_missing_text_is_refused() {
    let blank: Run = cli(&["feedback", "add", " "]);
    assert_eq!(blank.code, exit::INVALID);
    assert!(blank.err.starts_with("✗ the feedback is empty\n  "), "{}", blank.err);
    assert_eq!(cli(&["feedback", "add"]).code, exit::USAGE);
}
