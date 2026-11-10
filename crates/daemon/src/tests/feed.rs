//! The canvas change feed through the real socket: a waiting client wakes on
//! another client's change, an empty feed succeeds, and a stop ends a wait.

use std::time::{Duration, Instant};

use serde_json::{Value, json};
use slinger_control::CanvasOps;
use tempfile::TempDir;

use super::Daemon;

const LONG: Duration = Duration::from_secs(30);

fn bx(x: i64) -> CanvasOps {
    let op = json!({ "op": "add", "item": { "page": "p1", "kind": "box", "x": x, "y": 0, "w": 10, "h": 10 } });
    CanvasOps { canvas: "main".into(), by: "user".into(), ops: op }
}

fn numbers(ops: &[Value]) -> Vec<u64> {
    ops.iter().map(|op| op["n"].as_u64().unwrap()).collect()
}

#[test]
fn a_waiting_client_gets_the_operation_another_client_applies() {
    let daemon = Daemon::start();
    let client = daemon.client();
    client.canvas_create("main").unwrap();
    client.canvas_apply(bx(0)).unwrap();
    let start = Instant::now();
    let feed = std::thread::scope(|scope| {
        let waiter = daemon.client();
        let waiting = scope.spawn(move || waiter.canvas_changes("main", 1, LONG));
        std::thread::sleep(Duration::from_millis(100));
        client.canvas_apply(bx(50)).unwrap();
        waiting.join().unwrap().unwrap()
    });
    assert!(start.elapsed() < LONG / 2, "the wait did not end on the change");
    assert_eq!((numbers(&feed.ops), feed.next), (vec![2], 3));
    assert_eq!((&feed.ops[0]["op"], &feed.ops[0]["item"]["x"]), (&json!("add"), &json!(50)));
    let all = client.canvas_changes("main", 0, Duration::ZERO).unwrap();
    assert_eq!((numbers(&all.ops), all.next), (vec![1, 2], 3));
    daemon.stop();
}

#[test]
fn after_the_newest_with_no_wait_is_empty_and_succeeds() {
    let daemon = Daemon::start();
    let client = daemon.client();
    client.canvas_create("main").unwrap();
    let empty = client.canvas_changes("main", 0, Duration::ZERO).unwrap();
    assert_eq!((empty.ops.len(), empty.next), (0, 1));
    client.canvas_apply(bx(0)).unwrap();
    let start = Instant::now();
    let none = client.canvas_changes("main", 1, Duration::from_millis(50)).unwrap();
    assert_eq!((none.ops.len(), none.next), (0, 2));
    assert!(start.elapsed() >= Duration::from_millis(50));

    let beyond = client.canvas_changes("main", 5, Duration::ZERO).unwrap_err();
    assert_eq!(beyond.code(), "invalid");
    assert!(beyond.hint().is_some());
    let missing = client.canvas_changes("gone", 0, Duration::ZERO).unwrap_err();
    assert_eq!(missing.code(), "not_found");
    daemon.stop();
}

#[test]
fn a_stop_ends_a_wait_with_an_empty_reply() {
    let daemon = Daemon::start();
    daemon.client().canvas_create("main").unwrap();
    let waiter = daemon.client();
    let start = Instant::now();
    let waiting = std::thread::spawn(move || waiter.canvas_changes("main", 0, LONG));
    std::thread::sleep(Duration::from_millis(100));
    daemon.stop();
    let feed = waiting.join().unwrap().unwrap();
    assert_eq!((feed.ops.len(), feed.next), (0, 1));
    assert!(start.elapsed() < LONG / 2, "the stop waited out the wait");
}

#[test]
fn the_feed_serves_fields_this_daemon_does_not_know() {
    let data = TempDir::new().unwrap();
    let dir = data.path().join("canvases");
    std::fs::create_dir_all(&dir).unwrap();
    let file = concat!(
        "{\"slinger\":\"canvas\",\"v\":1,\"id\":\"main\",\"first\":1}\n",
        "{\"n\":1,\"t\":\"2025-11-10T16:00:00.000Z\",\"by\":\"user\",\"op\":\"page.add\",\"page\":\"p2\",\"name\":\"x\",\"tint\":\"blue\"}\n",
    );
    std::fs::write(dir.join("main.canvas"), file).unwrap();
    let daemon = Daemon::start_in(data.path());
    let client = daemon.client();
    let feed = client.canvas_changes("main", 0, Duration::ZERO).unwrap();
    assert_eq!(feed.ops[0]["tint"], "blue");
    client.canvas_apply(bx(0)).unwrap();
    let raw = daemon.raw(b"{\"v\":1,\"op\":\"canvas_changes\",\"canvas\":\"main\",\"after\":1}\n");
    let reply: Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(
        (&reply["ok"], &reply["next"], &reply["ops"][0]["n"]),
        (&json!(true), &json!(3), &json!(2))
    );
    daemon.stop();
}
