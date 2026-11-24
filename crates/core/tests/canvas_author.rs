//! The user's writing stays the user's (docs/design/writing-column.md,
//! Question 5): an agent cannot change a text the user wrote by any verb,
//! the user's edit of an agent's words makes that text the user's, and an
//! agent's text is at most `AGENT_ROWS` rows, so a reply stays short.
#![cfg(test)]

use serde_json::{Value, json};
use slinger_core::canvas::file::{AGENT_ROWS, Edit, Log, Put};
use slinger_core::canvas::{Author, Time};

const AT: Time = Time(1_762_790_400_000);

fn scout() -> Author {
    Author::Agent("scout".into())
}

/// `n` rows of text, one a line.
fn rows(n: usize) -> String {
    (1..=n).map(|i| format!("row {i}")).collect::<Vec<_>>().join("\n")
}

/// A new log and the bytes of its file so far.
fn create() -> (Log, String) {
    Log::create("notes").unwrap()
}

fn add_text(
    log: &mut Log,
    bytes: &mut String,
    by: &Author,
    text: &str,
) -> slinger_core::Result<()> {
    let add =
        json!({"op": "add", "item": {"page": "p1", "kind": "text", "x": 0, "y": 0, "text": text}});
    bytes.push_str(&log.apply(by, AT, &add)?.bytes);
    Ok(())
}

fn item(log: &Log, id: &str) -> Value {
    let items = log.show(None).unwrap().items;
    items.into_iter().find(|i| i["id"] == id).unwrap()
}

#[test]
fn an_agent_cannot_change_a_text_the_user_wrote_by_any_verb() {
    let (mut log, mut bytes) = create();
    add_text(&mut log, &mut bytes, &Author::User, "my own words").unwrap();
    let boxed = json!({"op": "add", "item": {"page": "p1", "kind": "box", "x": 0, "y": 0, "w": 9000, "h": 9000}});
    log.apply(&Author::User, AT, &boxed).unwrap();
    let before = log.show(None).unwrap().items;
    let set = json!({"op": "set", "id": "i1", "fields": {"text": "an agent's words"}});
    let moved = json!({"op": "move", "ids": ["i1"], "dx": 10, "dy": 0});
    let removed = json!({"op": "remove", "ids": ["i1"]});
    let tries = [
        log.clone().apply(&scout(), AT, &set).map(drop),
        log.clone().apply(&scout(), AT, &moved).map(drop),
        log.clone().apply(&scout(), AT, &removed).map(drop),
        log.clone()
            .edit(&scout(), AT, "i1", &Edit { text: Some("mine now".into()), ..Edit::default() })
            .map(drop),
        log.clone()
            .edit(&scout(), AT, "i1", &Edit { to: Some([500, 0]), ..Edit::default() })
            .map(drop),
        log.clone().remove(&scout(), AT, "i1").map(drop),
        log.clone().fold(&scout(), AT, "i2", None).map(drop),
    ];
    for (n, tried) in tries.into_iter().enumerate() {
        let err = tried.expect_err(&format!("try {n} changed the user's text"));
        assert_eq!(err.code(), "invalid", "try {n}: {err}");
        assert!(err.to_string().contains("agent scout may not change"), "try {n}: {err}");
        assert!(err.hint().unwrap().contains("beside"), "try {n}: {:?}", err.hint());
    }
    // An agent may still answer beside it.
    log.put(&scout(), AT, "i1", &Put::Text("a question?".into())).unwrap();
    assert_eq!(log.show(None).unwrap().items[..2], before[..]);
}

#[test]
fn the_users_edit_of_an_agents_words_makes_the_text_the_users() {
    let (mut log, mut bytes) = create();
    add_text(&mut log, &mut bytes, &scout(), "a draft").unwrap();
    // A move, a resize or a recolour by the user leaves it the agent's.
    let moved = json!({"op": "move", "ids": ["i1"], "dx": 10, "dy": 0});
    bytes.push_str(&log.apply(&Author::User, AT, &moved).unwrap().bytes);
    assert_eq!(item(&log, "i1")["by"], "agent:scout");
    let polish = Edit { text: Some("a better draft".into()), ..Edit::default() };
    bytes.push_str(&log.edit(&scout(), AT, "i1", &polish).unwrap().written.bytes);
    // The user's new words make it the user's.
    let set = json!({"op": "set", "id": "i1", "fields": {"text": "my draft now"}});
    bytes.push_str(&log.apply(&Author::User, AT, &set).unwrap().bytes);
    assert_eq!(item(&log, "i1")["by"], "user");
    let err = log.edit(&scout(), AT, "i1", &polish).unwrap_err();
    assert_eq!(err.code(), "invalid", "{err}");
    // The log says so on its own: a reopened canvas reads it the same.
    let (reopened, _) = Log::open(bytes.as_bytes()).unwrap();
    assert_eq!(item(&reopened, "i1")["by"], "user");
    assert_eq!(item(&reopened, "i1")["text"], "my draft now");
}

#[test]
fn an_agents_text_is_at_most_twelve_rows() {
    assert_eq!(AGENT_ROWS, 12);
    let (mut log, mut bytes) = create();
    add_text(&mut log, &mut bytes, &Author::User, &rows(100)).unwrap();
    // Twelve rows go; thirteen do not, by put, apply or edit.
    let placed = log.put(&scout(), AT, "i1", &Put::Text(rows(AGENT_ROWS))).unwrap();
    let mine = placed.written.made[0].clone();
    let too_long = rows(AGENT_ROWS + 1);
    let set = json!({"op": "set", "id": mine, "fields": {"text": too_long}});
    let tries = [
        log.clone().put(&scout(), AT, "i1", &Put::Text(too_long.clone())).map(drop),
        add_text(&mut log.clone(), &mut String::new(), &scout(), &too_long),
        log.clone().apply(&scout(), AT, &set).map(drop),
        log.clone()
            .edit(&scout(), AT, &mine, &Edit { text: Some(too_long.clone()), ..Edit::default() })
            .map(drop),
    ];
    for (n, tried) in tries.into_iter().enumerate() {
        let err = tried.expect_err(&format!("try {n} wrote a long text"));
        assert_eq!(err.code(), "too_long", "try {n}: {err}");
        assert_eq!(err.hint(), Some("split it: one short put beside each subject"));
    }
    // A row is a row at the text's column: one long line wraps into many.
    let words = vec!["word"; 200].join(" ");
    let err = log.clone().put(&scout(), AT, "i1", &Put::Text(words)).unwrap_err();
    assert_eq!(err.code(), "too_long", "{err}");
    // The user's texts have no such limit.
    log.put(&Author::User, AT, "i1", &Put::Text(rows(40))).unwrap();
}

#[test]
fn an_agents_older_long_text_still_opens_moves_and_shortens() {
    let (mut log, mut bytes) = create();
    add_text(&mut log, &mut bytes, &Author::User, &rows(30)).unwrap();
    // A file written before the limit: the agent's text is 30 rows.
    let old = bytes.replace("\"by\":\"user\"", "\"by\":\"agent:scout\"");
    assert_ne!(old, bytes);
    let (mut log, _) = Log::open(old.as_bytes()).unwrap();
    assert_eq!(item(&log, "i1")["by"], "agent:scout");
    let moved = json!({"op": "move", "ids": ["i1"], "dx": 10, "dy": 0});
    log.apply(&scout(), AT, &moved).unwrap();
    let longer = json!({"op": "set", "id": "i1", "fields": {"text": rows(31)}});
    assert_eq!(log.clone().apply(&scout(), AT, &longer).unwrap_err().code(), "too_long");
    let shorter = json!({"op": "set", "id": "i1", "fields": {"text": rows(3)}});
    log.apply(&scout(), AT, &shorter).unwrap();
}
