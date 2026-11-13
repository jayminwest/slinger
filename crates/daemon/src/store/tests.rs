//! The store against a real SQLite file in a temporary directory.

use std::path::PathBuf;

use rusqlite::Connection;
use slinger_core::record::{self, BlobId, Body, Event, Tags};

use super::{FILE, Query, Store, StoreError, machine_from};

const MACHINE: &str = "test-machine";

/// An event of `kind` (known or not) with a machine tag the store must replace.
fn event(kind: &str, workspace: Option<&str>) -> Event {
    let mut tags = Tags::on("not-this-machine");
    tags.workspace = workspace.map(str::to_owned);
    let body = Body::Other { kind: kind.to_owned(), payload: serde_json::json!({}) };
    // Through the format, so a known kind comes back typed as the store returns it.
    record::decode(&record::encode(&Event::new(7, "test", tags, body))).unwrap()
}

fn numbers(events: &[Event]) -> Vec<u64> {
    events.iter().map(|e| e.number.unwrap()).collect()
}

#[test]
fn appended_events_survive_a_reopen_in_order() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path(), MACHINE).unwrap();
    let before = super::now_ms();
    let first = store.append(event("agent_done", None)).unwrap();
    let second = store.append(event("agent_blocked_later", Some("w"))).unwrap();
    let third = store.append(event("agent_done", None)).unwrap();
    let after = super::now_ms();
    assert_eq!(numbers(&[first.clone(), second, third]), [1, 2, 3]);
    assert!((before..=after).contains(&first.time), "{} not in {before}..={after}", first.time);
    drop(store);

    let store = Store::open(dir.path(), MACHINE).unwrap();
    let all = store.events(&Query::default()).unwrap();
    assert_eq!(numbers(&all), [1, 2, 3]);
    assert_eq!(all[0], first);
    assert_eq!(numbers(&[store.append(event("agent_done", None)).unwrap()]), [4]);
}

#[test]
fn the_machine_name_is_on_every_stored_event() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path(), MACHINE).unwrap();
    store.append(event("agent_done", None)).unwrap();
    store.append(event("something_new", Some("w"))).unwrap();
    let all = store.events(&Query::default()).unwrap();
    assert_eq!(all.len(), 2);
    assert!(all.iter().all(|e| e.tags.machine == MACHINE));
}

#[test]
fn the_record_keeps_its_machine_name_when_the_host_is_renamed() {
    let dir = tempfile::tempdir().unwrap();
    drop(Store::open(dir.path(), MACHINE).unwrap());
    let store = Store::open(dir.path(), "renamed-host").unwrap();
    assert_eq!(store.machine(), MACHINE);
    let added = store.append(event("agent_done", None)).unwrap();
    assert_eq!(added.tags.machine, MACHINE);
    let mine = Query { machine: Some(MACHINE.into()), ..Query::default() };
    assert_eq!(numbers(&store.events(&mine).unwrap()), [1]);
}

/// A data directory with a record laid out by a layout-1 slingerd, which
/// kept no machine name, holding one event per name in `machines`.
fn layout_1_record(machines: &[&str]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let db = Connection::open(dir.path().join(FILE)).unwrap();
    db.execute_batch(super::schema::V1).unwrap();
    db.pragma_update(None, "user_version", 1).unwrap();
    for (number, machine) in (1..).zip(machines) {
        db.execute(
            "INSERT INTO events (number, time, kind, machine, event) VALUES (?1, 0, 'k', ?2, '{}')",
            rusqlite::params![number, machine],
        )
        .unwrap();
    }
    dir
}

#[test]
fn a_layout_1_record_keeps_the_name_on_its_first_event() {
    let dir = layout_1_record(&["old-host", "other"]);
    let store = Store::open(dir.path(), MACHINE).unwrap();
    assert_eq!(store.machine(), "old-host");
    drop(store);
    assert_eq!(Store::open(dir.path(), "third").unwrap().machine(), "old-host");
    let empty = layout_1_record(&[]);
    assert_eq!(Store::open(empty.path(), MACHINE).unwrap().machine(), MACHINE);
}

#[test]
fn reads_by_range_kind_tag_and_limit() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path(), MACHINE).unwrap();
    for (kind, workspace) in [
        ("agent_done", Some("a")),
        ("pane_later", Some("b")),
        ("agent_done", Some("b")),
        ("pane_later", None),
        ("agent_done", Some("a")),
    ] {
        store.append(event(kind, workspace)).unwrap();
    }
    let read = |query: Query| numbers(&store.events(&query).unwrap());
    assert_eq!(read(Query { from: Some(2), to: Some(4), ..Query::default() }), [2, 3, 4]);
    assert_eq!(read(Query { from: Some(4), ..Query::default() }), [4, 5]);
    assert_eq!(read(Query { to: Some(1), ..Query::default() }), [1]);
    assert_eq!(read(Query { kind: Some("pane_later".into()), ..Query::default() }), [2, 4]);
    assert_eq!(read(Query { workspace: Some("b".into()), ..Query::default() }), [2, 3]);
    let both =
        Query { kind: Some("agent_done".into()), workspace: Some("a".into()), ..Query::default() };
    assert_eq!(read(both), [1, 5]);
    assert_eq!(read(Query { limit: Some(2), from: Some(2), ..Query::default() }), [2, 3]);
    assert_eq!(read(Query { agent: Some("none".into()), ..Query::default() }), [0u64; 0]);
    assert_eq!(read(Query { canvas: Some("none".into()), ..Query::default() }), [0u64; 0]);
    assert_eq!(read(Query { from: Some(9), to: Some(2), ..Query::default() }), [0u64; 0]);
    assert_eq!(read(Query { from: Some(u64::MAX), ..Query::default() }), [0u64; 0]);
}

#[test]
fn an_event_is_stored_as_encode_wrote_it_unknown_fields_and_all() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path(), MACHINE).unwrap();
    let newer = record::decode(
        r#"{"time":1,"kind":"agent_done","version":1,"source":"s","tags":{"machine":"m","room":"r"},"payload":{"mood":"ok"},"extra":[1]}"#,
    )
    .unwrap();
    let stored = store.append(newer).unwrap();
    let raw: String = Connection::open(dir.path().join(FILE))
        .unwrap()
        .query_row("SELECT event FROM events", [], |row| row.get(0))
        .unwrap();
    assert_eq!(raw, record::encode(&stored));
    for field in [r#""room":"r""#, r#""mood":"ok""#, r#""extra":[1]"#] {
        assert!(raw.contains(field), "{raw}");
    }
    assert_eq!(store.events(&Query::default()).unwrap(), [stored]);
}

#[test]
fn the_log_refuses_updates_and_deletes() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path(), MACHINE).unwrap();
    store.append(event("agent_done", None)).unwrap();
    store.put_blob(b"x").unwrap();
    let db = Connection::open(dir.path().join(FILE)).unwrap();
    for sql in
        ["UPDATE events SET kind = 'x'", "DELETE FROM events", "UPDATE blobs SET bytes = x'00'"]
    {
        let err = db.execute(sql, []).unwrap_err();
        assert!(
            err.to_string().contains("append-only") || err.to_string().contains("never changes"),
            "{err}"
        );
    }
    assert_eq!(store.events(&Query::default()).unwrap().len(), 1);
}

#[test]
fn putting_the_same_bytes_twice_gives_one_row() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path(), MACHINE).unwrap();
    let id = store.put_blob(b"packet").unwrap();
    assert_eq!(store.put_blob(b"packet").unwrap(), id);
    assert_eq!(id, BlobId::of(b"packet"));
    let other = store.put_blob(b"").unwrap();
    let rows: i64 = Connection::open(dir.path().join(FILE))
        .unwrap()
        .query_row("SELECT COUNT(*) FROM blobs", [], |row| row.get(0))
        .unwrap();
    assert_eq!(rows, 2);
    drop(store);

    let store = Store::open(dir.path(), MACHINE).unwrap();
    assert_eq!(store.blob(id).unwrap().as_deref(), Some(&b"packet"[..]));
    assert_eq!(store.blob(other).unwrap().as_deref(), Some(&b""[..]));
    assert_eq!(store.blob(BlobId::of(b"never put")).unwrap(), None);
}

#[test]
fn a_record_from_a_newer_slingerd_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    drop(Store::open(dir.path(), MACHINE).unwrap());
    Connection::open(dir.path().join(FILE))
        .unwrap()
        .pragma_update(None, "user_version", 9)
        .unwrap();
    let err = Store::open(dir.path(), MACHINE).unwrap_err();
    assert!(matches!(err, StoreError::Newer { found: 9, .. }), "{err:?}");
    assert_eq!(err.code(), "conflict");
    assert!(err.hint().unwrap().contains("newer slingerd"));
}

#[test]
fn a_data_dir_that_cannot_be_made_is_an_error_with_a_hint() {
    let err = Store::open(&PathBuf::from("/dev/null/slinger"), MACHINE).unwrap_err();
    assert!(matches!(err, StoreError::Dir { .. }), "{err:?}");
    assert_eq!(err.code(), "internal");
    assert!(err.to_string().starts_with("cannot create data directory /dev/null/slinger"));
    assert!(err.hint().unwrap().contains("--data-dir"));
}

#[test]
fn a_held_data_dir_is_a_conflict_until_its_store_is_gone() {
    let dir = tempfile::tempdir().unwrap();
    let first = Store::open(dir.path(), MACHINE).unwrap();
    let err = Store::open(dir.path(), MACHINE).unwrap_err();
    assert!(matches!(err, StoreError::Taken(_)), "{err:?}");
    assert_eq!(err.code(), "conflict");
    let message = format!("data directory {} is in use by another slingerd", dir.path().display());
    assert_eq!(err.to_string(), message);
    assert_eq!(err.hint().unwrap(), "stop that daemon, or pass another --data-dir");
    drop(first);
    let again = Store::open(dir.path(), MACHINE).unwrap();
    assert_eq!(again.events(&Query::default()).unwrap(), []);
}

#[test]
fn a_second_slingerd_on_a_held_data_dir_exits_4_and_leaves_the_record_alone() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path(), MACHINE).unwrap();
    store.append(event("agent_done", None)).unwrap();
    // Its own socket, so only the data directory is shared.
    let socket = dir.path().join("other.sock");
    let args = ["slingerd", "--socket", socket.to_str().unwrap(), "--data-dir"];
    let args = args.into_iter().chain([dir.path().to_str().unwrap()]);
    let (mut out, mut err) = (Vec::new(), Vec::new());
    assert_eq!(crate::run(args, &mut out, &mut err), 4);
    let err = String::from_utf8(err).unwrap();
    assert_eq!(String::from_utf8(out).unwrap(), "");
    assert!(err.starts_with("✗ data directory "), "{err}");
    assert!(err.contains(" is in use by another slingerd\n  stop that daemon"), "{err}");
    assert_eq!(numbers(&store.events(&Query::default()).unwrap()), [1]);
}

#[test]
fn a_file_that_is_not_a_database_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join(FILE), vec![b'x'; 4096]).unwrap();
    let err = Store::open(dir.path(), MACHINE).unwrap_err();
    assert!(matches!(err, StoreError::Open { .. }), "{err:?}");
    assert!(err.hint().is_some());
}

#[test]
fn an_unreadable_stored_event_is_reported_by_number() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path(), MACHINE).unwrap();
    Connection::open(dir.path().join(FILE))
        .unwrap()
        .execute(
            "INSERT INTO events (number, time, kind, machine, event) VALUES (1, 0, 'k', 'm', 'nope')",
            [],
        )
        .unwrap();
    let err = store.events(&Query::default()).unwrap_err();
    assert!(matches!(err, StoreError::Unreadable { number: 1, .. }), "{err:?}");
    assert_eq!((err.code(), err.hint()), ("internal", None));
}

#[test]
fn no_data_dir_is_invalid_with_a_hint() {
    let none = StoreError::NoDataDir;
    assert_eq!(
        (none.code(), none.hint().unwrap().as_str()),
        ("invalid", "set HOME, or pass --data-dir")
    );
}

#[test]
fn the_machine_is_the_host_name_up_to_its_first_dot() {
    assert_eq!(machine_from("my-laptop.local"), "my-laptop");
    assert_eq!(machine_from("my-laptop"), "my-laptop");
    assert_eq!(machine_from(""), "localhost");
    assert_eq!(machine_from(".local"), "localhost");
    assert_ne!(super::machine_name(), "");
}

#[test]
fn append_if_sees_what_it_selects_and_appends_only_on_ok() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path(), MACHINE).unwrap();
    store.append(event("a", None)).unwrap();
    store.append(event("b", None)).unwrap();
    let only_a = Query { kind: Some("a".into()), ..Query::default() };
    let refused = store.append_if(&only_a, |seen| Err(numbers(seen))).unwrap();
    assert_eq!(refused, Err(vec![1]));
    let added =
        store.append_if(&only_a, |seen| Ok::<_, ()>(event("a", Some(&seen.len().to_string()))));
    let added = added.unwrap().unwrap();
    assert_eq!((added.number, added.tags.machine.as_str()), (Some(3), MACHINE));
    assert_eq!(added.tags.workspace.as_deref(), Some("1"));
    assert_eq!(numbers(&store.events(&Query::default()).unwrap()), [1, 2, 3]);
}
