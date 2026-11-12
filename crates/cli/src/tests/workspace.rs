//! `slinger workspace ...` against a real daemon.

use super::{cli_each, cli_on, json, with_daemon};
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

#[test]
fn a_broken_file_is_listed_with_its_error_and_show_fails_with_it() {
    let projects = tempfile::tempdir().unwrap();
    let (good, bad) = (projects.path().join("good"), projects.path().join("bad"));
    let runs = with_daemon(|socket| {
        let mut runs = Vec::new();
        for dir in [&good, &bad] {
            std::fs::create_dir(dir).unwrap();
            runs.push(cli_on(socket, &["workspace", "open", dir.to_str().unwrap()]));
        }
        let rules = "version = 1\nname = \"bad\"\n[agent]\nname = \"a\"\ntools = [\"bash\"]\n";
        std::fs::write(bad.join("slinger-workspace.toml"), rules).unwrap();
        runs.push(cli_on(socket, &["workspace", "list"]));
        runs.push(cli_on(socket, &["workspace", "show", "bad"]));
        runs.push(cli_on(socket, &["workspace", "list", "--json"]));
        runs
    });
    assert_eq!(runs[2].code, exit::OK, "{}", runs[2].err);
    let lines: Vec<&str> = runs[2].out.lines().collect();
    assert_eq!(lines.len(), 2, "{}", runs[2].out);
    assert!(lines[0].starts_with("bad\t-\t/") && lines[0].contains("\t✗ /"), "{}", lines[0]);
    assert!(lines[0].contains("agent.tools: \"bash\""), "{}", lines[0]);
    assert!(lines[1].starts_with("good\tgood\t/"), "{}", lines[1]);
    assert_eq!(runs[3].code, exit::INVALID);
    assert!(runs[3].err.contains("did you mean \"Bash\"?"), "{}", runs[3].err);
    let error = &json(&runs[4])["workspaces"][0]["error"];
    assert_eq!((&error["code"], &error["field"]), (&"invalid".into(), &"agent.tools".into()));
}
