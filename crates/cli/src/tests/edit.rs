//! `slinger canvas edit`, `remove`, `fold` and `unfold` against a real daemon.

use super::{cli_each, json};
use crate::output::exit;

const BOX: &str = r#"{"op":"add","item":{"page":"p1","kind":"box","x":0,"y":0,"w":1000,"h":500}}"#;

#[test]
fn edit_prints_the_item_and_its_new_box_and_remove_prints_nothing_twice() {
    let put = ["canvas", "put", "main", "--beside", "i1", "--text", "hi", "--agent", "scout"];
    let runs = cli_each(&[
        &["canvas", "create", "main"],
        &["canvas", "apply", "main", BOX],
        &put,
        &["canvas", "edit", "main", "--item", "i2", "--move", "0,900", "--agent", "scout"],
        &[
            "canvas", "edit", "main", "--item", "i2", "--box", "300,200", "--agent", "scout",
            "--json",
        ],
        &["canvas", "remove", "main", "--item", "i2", "--agent", "scout"],
        &["canvas", "remove", "main", "--item", "i2", "--agent", "scout", "--json"],
    ]);
    for r in &runs {
        assert_eq!((r.code, r.err.as_str()), (exit::OK, ""), "{}", r.out);
    }
    assert_eq!(runs[3].out, "i2\t0,900,240,240\n");
    let edited = json(&runs[4]);
    assert_eq!(
        (&edited["command"], &edited["w"], &edited["seq"]),
        (&"canvas edit".into(), &300.into(), &serde_json::json!([4]))
    );
    assert_eq!(runs[5].out, "");
    let removed = json(&runs[6]);
    assert_eq!(
        (&removed["command"], &removed["seq"]),
        (&"canvas remove".into(), &serde_json::json!([]))
    );
}

#[test]
fn an_edit_of_the_users_item_or_with_nothing_to_change_gets_its_exit_code() {
    let runs = cli_each(&[
        &["canvas", "create", "main"],
        &["canvas", "apply", "main", BOX],
        &["canvas", "edit", "main", "--item", "i1", "--move", "0,0", "--agent", "scout"],
        &["canvas", "remove", "main", "--item", "i1", "--agent", "scout"],
        &["canvas", "edit", "main", "--item", "i1", "--agent", "scout"],
        &["canvas", "edit", "main", "--item", "i1", "--move", "3", "--agent", "scout"],
        &["canvas", "edit", "main", "--item", "i9", "--move", "0,0", "--agent", "scout"],
    ]);
    let codes: Vec<u8> = runs.iter().map(|r| r.code).collect();
    assert_eq!(
        codes,
        [
            exit::OK,
            exit::OK,
            exit::INVALID,
            exit::INVALID,
            exit::USAGE,
            exit::USAGE,
            exit::NOT_FOUND
        ]
    );
    assert!(runs[2].err.contains("only items it made"), "{}", runs[2].err);
}

#[test]
fn fold_and_unfold_print_nothing_and_a_taken_room_is_a_conflict() {
    let inside =
        r#"{"op":"add","item":{"page":"p1","kind":"box","x":100,"y":100,"w":200,"h":200}}"#;
    let later = r#"{"op":"add","item":{"page":"p1","kind":"box","x":800,"y":300,"w":100,"h":100}}"#;
    let runs = cli_each(&[
        &["canvas", "create", "main"],
        &["canvas", "apply", "main", BOX],
        &["canvas", "apply", "main", inside],
        &["canvas", "fold", "main", "b1"],
        &["canvas", "unfold", "main", "i1", "--json"],
        &["canvas", "fold", "main", "i1", "--json"],
        &["canvas", "apply", "main", later],
        &["canvas", "unfold", "main", "i1"],
        &["canvas", "fold", "main", "i3", "--agent", "scout"],
    ]);
    let codes: Vec<u8> = runs.iter().map(|r| r.code).collect();
    assert_eq!(codes[..7], [exit::OK; 7]);
    assert_eq!(codes[7..], [exit::CONFLICT, exit::INVALID]);
    assert_eq!(runs[3].out, "");
    let opened = json(&runs[4]);
    assert_eq!(
        (&opened["command"], &opened["item"], &opened["w"], &opened["seq"]),
        (&"canvas unfold".into(), &"i1".into(), &1000.into(), &serde_json::json!([4]))
    );
    let folded = json(&runs[5]);
    assert_eq!(
        (&folded["command"], &folded["seq"]),
        (&"canvas fold".into(), &serde_json::json!([5]))
    );
    assert!(folded["w"].as_i64().unwrap() < 1000, "{folded}");
    assert!(runs[7].err.contains("i3"), "{}", runs[7].err);
}
