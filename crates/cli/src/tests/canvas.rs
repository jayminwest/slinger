//! `slinger canvas ...` against a real daemon.

use super::{cli, cli_as, cli_bare, cli_each, cli_on, cli_raw, json, with_daemon};
use crate::output::exit;

const BOX: &str = r#"{"op":"add","item":{"page":"p1","kind":"box","x":10,"y":20,"w":100,"h":50}}"#;

#[test]
fn create_apply_show_and_list_print_their_results() {
    let runs = cli_each(&[
        &["canvas", "create", "main"],
        &["canvas", "apply", "main", BOX],
        &["canvas", "apply", "main", BOX, "--agent", "ada"],
        &["canvas", "show", "main"],
        &["canvas", "list"],
        &["canvas", "undo", "main"],
        &["canvas", "show", "main", "--page", "Page 1"],
    ]);
    for r in &runs {
        assert_eq!((r.code, r.err.as_str()), (exit::OK, ""), "{}", r.out);
    }
    let out: Vec<&str> = runs.iter().map(|r| r.out.as_str()).collect();
    assert_eq!(out[..3], ["", "i1\n", "i2\n"]);
    let lines: Vec<&str> = out[3].lines().collect();
    assert_eq!(lines.len(), 2);
    assert!(lines[0].starts_with("i1\tbox\tuser\t{"), "{}", lines[0]);
    assert!(lines[1].starts_with("i2\tbox\tagent:ada\t"), "{}", lines[1]);
    assert_eq!(out[4], "main\n");
    assert_eq!(out[5], "");
    assert!(out[6].starts_with("i2\t"), "{}", out[6]);
}

/// The agent comes from the environment the run is given, `--agent` before
/// it; with none, the user acts, whatever this test process's own
/// `SLINGER_AGENT` says (slinger-09c5b5).
#[test]
fn the_agent_comes_from_the_runs_environment_and_the_flag_wins() {
    let shown = with_daemon(|socket| {
        assert_eq!(cli_on(socket, &["canvas", "create", "main"]).code, exit::OK);
        for run in [
            cli_on(socket, &["canvas", "apply", "main", BOX]),
            cli_as("scout", socket, &["canvas", "apply", "main", BOX]),
            cli_as("scout", socket, &["canvas", "apply", "main", BOX, "--agent", "ada"]),
            cli_as("scout", socket, &["canvas", "put", "main", "--beside", "i1", "--text", "hi"]),
        ] {
            assert_eq!((run.code, run.err.as_str()), (exit::OK, ""), "{}", run.out);
        }
        cli_on(socket, &["canvas", "show", "main"]).out
    });
    let by: Vec<&str> = shown.lines().map(|l| l.split('\t').nth(2).unwrap()).collect();
    assert_eq!(by, ["user", "agent:scout", "agent:ada", "agent:scout"]);
}

#[test]
fn json_carries_the_page_and_the_change() {
    let runs = cli_each(&[
        &["canvas", "create", "main", "--json"],
        &["canvas", "apply", "main", BOX, "--json"],
        &["canvas", "show", "main", "--json"],
        &["canvas", "list", "--json"],
    ]);
    let docs: Vec<_> = runs.iter().map(json).collect();
    assert_eq!(
        (&docs[0]["command"], &docs[0]["canvas"]),
        (&"canvas create".into(), &"main".into())
    );
    assert_eq!(docs[1]["seq"], serde_json::json!([1]));
    assert_eq!(docs[1]["made"], serde_json::json!(["i1"]));
    assert_eq!((&docs[2]["page"], &docs[2]["next"]), (&"p1".into(), &2.into()));
    assert_eq!(docs[2]["items"][0]["x"], 10);
    assert_eq!(docs[3]["canvases"], serde_json::json!(["main"]));
}

#[test]
fn operations_can_come_from_standard_input() {
    with_daemon(|socket| {
        assert_eq!(cli_on(socket, &["canvas", "create", "main"]).code, exit::OK);
        let r = cli_raw(socket, &["canvas", "apply", "main", "-"], BOX.as_bytes(), &mut Vec::new());
        assert_eq!((r.code, r.out.as_str()), (exit::OK, "i1\n"), "{}", r.err);
    });
}

#[test]
fn failures_map_to_their_exit_codes() {
    let r = cli(&["canvas", "show", "nope"]);
    assert_eq!((r.code, r.out.as_str()), (exit::NOT_FOUND, ""));
    assert!(r.err.contains("canvas create nope"), "{}", r.err);
    let runs = cli_each(&[
        &["canvas", "create", "main"],
        &["canvas", "create", "main"],
        &["canvas", "apply", "main", "{not json"],
        &["canvas", "create", "Main"],
        &["canvas", "apply", "main", BOX],
        &["canvas", "apply", "main", r#"{"op":"remove","ids":["i1"]}"#, "--agent", "ada"],
    ]);
    let codes: Vec<u8> = runs.iter().map(|r| r.code).collect();
    assert_eq!(
        codes,
        [exit::OK, exit::CONFLICT, exit::INVALID, exit::INVALID, exit::OK, exit::INVALID]
    );
    assert!(runs[5].err.contains("agent ada may not change"), "{}", runs[5].err);
}

#[test]
fn create_with_ops_makes_the_whole_canvas_or_none() {
    let dir = tempfile::TempDir::new().unwrap();
    let file = |name: &str, text: &str| {
        let path = dir.path().join(name);
        std::fs::write(&path, text).unwrap();
        path.to_str().unwrap().to_string()
    };
    let text = r#"{"op":"add","item":{"page":"p1","kind":"text","x":0,"y":0,"text":"plan"}}"#;
    let two = file("two.json", &format!("[{BOX},{text}]"));
    // Its second box is on a page the canvas does not have.
    let refused = file("refused.json", &format!("[{BOX},{}]", BOX.replace("p1", "p9")));
    let not_json = file("not.json", "{not json");
    let gone = dir.path().join("gone.json");
    let runs = cli_each(&[
        &["canvas", "create", "plan", "--ops", &two],
        &["canvas", "show", "plan"],
        &["canvas", "create", "half", "--ops", &refused],
        &["canvas", "create", "bad", "--ops", &not_json, "--json"],
        &["canvas", "create", "none", "--ops", gone.to_str().unwrap()],
        &["canvas", "list"],
    ]);
    let codes: Vec<u8> = runs.iter().map(|r| r.code).collect();
    assert_eq!(
        codes,
        [exit::OK, exit::OK, exit::NOT_FOUND, exit::INVALID, exit::NOT_FOUND, exit::OK]
    );
    assert_eq!((runs[0].out.as_str(), runs[0].err.as_str()), ("", ""));
    let lines: Vec<&str> = runs[1].out.lines().collect();
    assert_eq!(lines.len(), 2, "{}", runs[1].out);
    assert!(lines[0].starts_with("i1\tbox\tuser\t{"), "{}", lines[0]);
    assert!(lines[1].starts_with("i2\ttext\tuser\t{"), "{}", lines[1]);
    assert_eq!(json(&runs[3])["code"], "invalid");
    // Only the canvas whose every operation applied is there.
    assert_eq!(runs[5].out, "plan\n");
}

const BOARD: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../core/tests/fixtures/idea.tldraw");

#[test]
fn import_makes_a_canvas_whose_pages_show() {
    let runs = cli_each(&[
        &["canvas", "import", BOARD],
        &["canvas", "show", "idea", "--json"],
        &["canvas", "import", BOARD, "--name", "board", "--json"],
    ]);
    assert_eq!((runs[0].code, runs[0].err.as_str()), (exit::OK, ""));
    assert_eq!(
        runs[0].out,
        "idea\npages\t5\narrow\t57\nbox\t80\nstroke\t7563\ntext\t125\n\
         skipped\tgeo:diamond\t1\nskipped\tgeo:ellipse\t2\nskipped\tgroup\t8\nskipped\tline\t4\n"
    );
    let shown = json(&runs[1]);
    let names: Vec<_> = shown["pages"].as_array().unwrap().iter().map(|p| &p["name"]).collect();
    assert_eq!(names, ["inital-planning", "claude", "slinger-diagrams", "fable-5.2", "spec"]);
    let imported = json(&runs[2]);
    assert_eq!(
        (&imported["command"], &imported["canvas"]),
        (&"canvas import".into(), &"board".into())
    );
    assert_eq!((&imported["pages"], &imported["items"]["stroke"]), (&5.into(), &7563.into()));
    assert_eq!(imported["skipped"]["group"], 8);
}

#[test]
fn import_of_a_missing_or_wrong_file_fails_with_its_code() {
    let dir = tempfile::TempDir::new().unwrap();
    let wrong = dir.path().join("notes.tldraw");
    std::fs::write(&wrong, "just some text").unwrap();
    let missing = dir.path().join("gone.tldraw");
    let runs = cli_each(&[
        &["canvas", "import", missing.to_str().unwrap()],
        &["canvas", "import", wrong.to_str().unwrap(), "--json"],
    ]);
    assert_eq!((runs[0].code, runs[0].out.as_str()), (exit::NOT_FOUND, ""));
    assert_eq!(runs[1].code, exit::INVALID);
    assert_eq!(json(&runs[1])["code"], "invalid");
}

#[test]
fn schema_needs_no_daemon_and_names_every_kind_apply_takes() {
    let text = cli_bare(&["canvas", "schema"]);
    assert_eq!((text.code, text.err.as_str()), (exit::OK, ""));
    assert!(text.out.lines().all(|l| l.split('\t').count() == 3), "{}", text.out);
    let doc = json(&cli_bare(&["canvas", "schema", "--json"]));
    let kinds: Vec<&String> = doc["kinds"].as_object().unwrap().keys().collect();
    assert_eq!(kinds, ["arrow", "box", "button", "live", "stroke", "text"]);
    assert!(doc["ops"]["page.remove"].is_string() && doc["units"]["pts"].is_string());
    let help = cli_bare(&["canvas", "apply", "--help"]);
    assert!(help.out.contains("kind\tarrow\tpts"), "{}", help.out);

    // Each kind as the schema gives its fields applies (a live item comes from 'ui open').
    let add = |item: &str| format!(r#"{{"op":"add","item":{{"page":"p1",{item}}}}}"#);
    let items = [
        r#""kind":"text","x":0,"y":0,"text":"plan""#,
        r#""kind":"box","x":0,"y":0,"w":100,"h":50"#,
        r#""kind":"arrow","pts":[0,0,100,0],"from":"i1","to":"i2""#,
        r#""kind":"stroke","width":20,"pts":[500,500,10,10,0,10]"#,
        r#""kind":"button","x":0,"y":900,"w":1600,"h":400"#,
    ];
    let ops = format!("[{}]", items.map(add).join(","));
    let runs = cli_each(&[&["canvas", "create", "main"], &["canvas", "apply", "main", &ops]]);
    assert_eq!(
        (runs[1].code, runs[1].out.as_str()),
        (exit::OK, "i1\ni2\ni3\ni4\ni5\n"),
        "{}",
        runs[1].err
    );
}
