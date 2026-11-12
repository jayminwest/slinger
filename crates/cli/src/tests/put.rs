//! `slinger canvas put` against a real daemon. The agent's name from
//! `SLINGER_AGENT` is tested against the real binary, in `tests/agent_env.rs`.

use super::{cli_each, json};
use crate::output::exit;

const BOX: &str = r#"{"op":"add","item":{"page":"p1","kind":"box","x":0,"y":0,"w":1000,"h":500}}"#;

#[test]
fn put_prints_the_new_item_and_its_box_and_it_shows_with_its_author() {
    let runs = cli_each(&[
        &["canvas", "create", "main"],
        &["canvas", "apply", "main", BOX],
        &["canvas", "put", "main", "--beside", "b1", "--text", "looks right", "--agent", "scout"],
        &["canvas", "show", "main"],
        &[
            "canvas", "put", "main", "--beside", "i1", "--box", "300,200", "--agent", "scout",
            "--json",
        ],
        &["canvas", "put", "main", "--beside", "i1", "--arrow", "i1", "i2", "--agent", "scout"],
    ]);
    for r in &runs {
        assert_eq!((r.code, r.err.as_str()), (exit::OK, ""), "{}", r.out);
    }
    assert_eq!(runs[2].out, "i2\t1240,0,2640,240\n");
    let shown: Vec<&str> = runs[3].out.lines().collect();
    assert!(shown[1].starts_with("i2\ttext\tagent:scout\t{"), "{}", shown[1]);
    let put = json(&runs[4]);
    assert_eq!(
        (&put["command"], &put["item"], &put["page"]),
        (&"canvas put".into(), &"i3".into(), &"p1".into())
    );
    assert_eq!((&put["w"], &put["h"], &put["seq"]), (&300.into(), &200.into(), &3.into()));
    assert!(runs[5].out.starts_with("i4\t"), "{}", runs[5].out);
}

#[test]
fn a_put_beside_an_unknown_item_or_with_a_bad_shape_gets_its_exit_code() {
    let runs = cli_each(&[
        &["canvas", "create", "main"],
        &["canvas", "apply", "main", BOX],
        &["canvas", "put", "main", "--beside", "i9", "--text", "hi", "--agent", "scout"],
        &["canvas", "put", "main", "--beside", "i1", "--agent", "scout"],
        &["canvas", "put", "main", "--beside", "i1", "--box", "3", "--agent", "scout"],
        &["canvas", "put", "main", "--beside", "i1", "--text", "a", "--box", "3,3", "--agent", "a"],
        &["canvas", "put", "main", "--beside", "i1", "--text", " ", "--agent", "scout"],
        &["canvas", "put", "main", "--beside", "i1", "--text", "hi", "--agent", " "],
    ]);
    let codes: Vec<u8> = runs.iter().map(|r| r.code).collect();
    assert_eq!(
        codes,
        [
            exit::OK,
            exit::OK,
            exit::NOT_FOUND,
            exit::USAGE,
            exit::USAGE,
            exit::USAGE,
            exit::INVALID,
            exit::USAGE
        ]
    );
    assert!(runs[2].err.contains("i9"), "{}", runs[2].err);
}

#[test]
fn put_takes_a_colour_ink_and_a_place_of_its_own() {
    let put = |more: &[&'static str]| {
        let head = ["canvas", "put", "main", "--beside", "i1", "--agent", "scout"];
        [&head[..], more].concat()
    };
    let runs = cli_each(&[
        &["canvas", "create", "main"],
        &["canvas", "apply", "main", BOX],
        &put(&["--text", "yes", "--color", "#ffd400", "--at", "-500,2000"]),
        &put(&["--ink", "0,0,400,0,0,-100", "--color", "#ff000080"]),
        &["canvas", "show", "main", "--json"],
        &put(&["--text", "x", "--color", "yellow"]),
        &put(&["--arrow", "i1", "i2", "--at", "0,0"]),
        &put(&["--ink", "0,0,400"]),
    ]);
    let codes: Vec<u8> = runs.iter().map(|r| r.code).collect();
    let errs: Vec<&str> = runs.iter().map(|r| r.err.as_str()).collect();
    assert_eq!(codes[..5], [exit::OK; 5], "{errs:?}");
    assert_eq!(codes[5..], [exit::INVALID, exit::USAGE, exit::USAGE]);
    assert!(runs[2].out.starts_with("i2\t-500,2000,"), "{}", runs[2].out);
    // Beside the box, a gap to its right: the stroke's box takes half its width.
    assert_eq!(runs[3].out, "i3\t1240,0,420,120\n");
    let items = &json(&runs[4])["items"];
    assert_eq!((&items[1]["color"], &items[1]["x"]), (&"#ffd400".into(), &(-500).into()));
    assert_eq!(
        (&items[2]["kind"], &items[2]["color"], &items[2]["width"]),
        (&"stroke".into(), &"#ff000080".into(), &20.into())
    );
    assert!(runs[5].err.contains("not a colour"), "{}", runs[5].err);
}

#[test]
fn put_and_edit_take_a_text_size_in_tenths() {
    let put = |more: &[&'static str]| {
        let head = ["canvas", "put", "main", "--beside", "i1", "--agent", "scout"];
        [&head[..], more].concat()
    };
    let edit = |more: &[&'static str]| {
        let head = ["canvas", "edit", "main", "--item", "i2", "--agent", "scout"];
        [&head[..], more].concat()
    };
    let runs = cli_each(&[
        &["canvas", "create", "main"],
        &["canvas", "apply", "main", BOX],
        &put(&["--text", "hi", "--size", "480", "--at", "0,1000"]),
        &edit(&["--size", "120"]),
        &put(&["--text", "hi", "--size", "4001"]),
        &edit(&["--size", "39"]),
        &put(&["--box", "3,3", "--size", "240"]),
        &put(&["--text", "hi", "--size", "big"]),
    ]);
    let codes: Vec<u8> = runs.iter().map(|r| r.code).collect();
    let errs: Vec<&str> = runs.iter().map(|r| r.err.as_str()).collect();
    assert_eq!(codes[..4], [exit::OK; 4], "{errs:?}");
    assert_eq!(codes[4..], [exit::INVALID, exit::INVALID, exit::INVALID, exit::USAGE]);
    assert!(runs[6].err.contains("only text"), "{}", runs[6].err);
    assert_eq!(runs[2].out, "i2\t0,1000,960,480\n", "two characters at 48 units");
    assert_eq!(runs[3].out, "i2\t0,1000,240,120\n");
    assert!(runs[4].err.contains("40 to 4000 tenths"), "{}", runs[4].err);
}
