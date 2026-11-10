//! `slinger workspace ...` against a real daemon.

use super::{cli_each, json};
use crate::output::exit;

#[test]
fn open_list_show_and_close_print_their_results() {
    let projects = tempfile::tempdir().unwrap();
    let dir = projects.path().join("notes");
    std::fs::create_dir(&dir).unwrap();
    let dir = dir.canonicalize().unwrap();
    let path = dir.to_str().unwrap();
    let runs = cli_each(&[
        &["workspace", "open", path],
        &["workspace", "open", "--name", "Loose Ends"],
        &["workspace", "list"],
        &["workspace", "show", "notes"],
        &["workspace", "close", path],
        &["workspace", "list"],
        &["canvas", "list"],
    ]);
    for r in &runs {
        assert_eq!((r.code, r.err.as_str()), (exit::OK, ""), "{}", r.out);
    }
    let out: Vec<&str> = runs.iter().map(|r| r.out.as_str()).collect();
    assert_eq!(out[..2], ["notes\n", "Loose Ends\n"]);
    let lines: Vec<&str> = out[2].lines().collect();
    assert!(lines[0].starts_with("Loose Ends\tloose-ends\t/"), "{}", lines[0]);
    assert_eq!(lines[1], format!("notes\tnotes\t{path}"));
    assert_eq!(
        out[3],
        format!(
            "name\tnotes\ntag\tnotes\ncanvas\tnotes\ndir\t{path}\nroot\tmy-laptop:{path} (primary)\n"
        )
    );
    assert_eq!(out[4], "");
    assert!(out[5].starts_with("Loose Ends\t") && out[5].lines().count() == 1, "{}", out[5]);
    assert_eq!(out[6], "loose-ends\nnotes\n");
    assert!(dir.join("slinger-workspace.toml").is_file());
}

#[test]
fn json_carries_the_workspace() {
    let runs = cli_each(&[
        &["workspace", "open", "--name", "scratch", "--json"],
        &["workspace", "open", "--name", "scratch", "--json"],
        &["workspace", "list", "--json"],
        &["workspace", "show", "scratch", "--json"],
        &["workspace", "close", "scratch", "--json"],
    ]);
    let docs: Vec<_> = runs.iter().map(json).collect();
    assert_eq!((&docs[0]["command"], &docs[0]["new"]), (&"workspace open".into(), &true.into()));
    assert_eq!(docs[1]["new"], false);
    let ws = &docs[0]["workspace"];
    assert_eq!((&ws["name"], &ws["canvas"]), (&"scratch".into(), &"scratch".into()));
    assert_eq!(docs[2]["workspaces"], serde_json::json!([ws]));
    assert_eq!(&docs[3]["workspace"], ws);
    assert_eq!(docs[4]["dir"], ws["dir"]);
}

#[test]
fn unknown_workspaces_and_missing_directories_get_their_exit_codes() {
    let runs = cli_each(&[
        &["workspace", "show", "nope"],
        &["workspace", "close", "./nope"],
        &["workspace", "open", "/no/such/dir"],
        &["workspace", "open"],
    ]);
    let codes: Vec<u8> = runs.iter().map(|r| r.code).collect();
    assert_eq!(codes, [exit::NOT_FOUND, exit::NOT_FOUND, exit::NOT_FOUND, exit::INVALID]);
    assert!(runs[0].err.contains("workspace list"), "{}", runs[0].err);
}
