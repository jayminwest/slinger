//! `canvas_read` through the real socket: the packet of new ink, stored as
//! blobs and written into a directory, and the last sling as its marker.

use std::fs;
use std::path::Path;

use serde_json::{Value, json};
use slinger_control::{CanvasOps, CanvasPacket, Client, NewTags, PacketAsk};
use tempfile::TempDir;

use super::{Daemon, new_event};

fn stroke(x: i64) -> Value {
    let item = json!({ "page": "p1", "kind": "stroke", "width": 20,
        "pts": [x, 0, 100, 50, 100, -50] });
    json!({ "op": "add", "item": item })
}

fn draw(client: &Client, x: i64) -> u64 {
    let ops = CanvasOps { canvas: "idea".into(), by: "user".into(), ops: stroke(x) };
    client.canvas_apply(ops).unwrap().seq[0]
}

fn read(client: &Client, after: Option<u64>) -> CanvasPacket {
    client.canvas_read(PacketAsk { canvas: "idea".into(), after, ..PacketAsk::default() }).unwrap()
}

/// The ids in one section of a packet's text.
fn section(text: &str, name: &str) -> Vec<String> {
    let heading = format!("# {name}");
    let lines = text.lines().skip_while(|l| *l != heading).skip(1);
    lines
        .take_while(|l| !l.starts_with("# "))
        .map(|l| l.split(' ').next().unwrap().into())
        .collect()
}

/// The bytes of a blob, through the socket.
fn blob(client: &Client, id: &str, scratch: &Path) -> Vec<u8> {
    let path = scratch.join("blob");
    client.blob_get(id, &path).unwrap();
    fs::read(path).unwrap()
}

#[test]
fn a_read_stores_the_packet_and_its_images_as_blobs_and_files() {
    let daemon = Daemon::start();
    let client = daemon.client();
    let scratch = TempDir::new().unwrap();
    client.canvas_create("idea").unwrap();
    assert_eq!(draw(&client, 0), 1);

    let first = read(&client, None);
    assert_eq!(section(&first.text, "new"), ["c1"]);
    assert_eq!(first.images.len(), 1);
    let header = first.text.lines().next().unwrap();
    let at = format!(" at={}", first.dir.display());
    assert!(header.starts_with("slinger-packet 1 canvas=idea page=Page%201 from=1 to=1 "));
    assert!(header.ends_with(&at), "{header}");
    // The cost of the packet as slung, its image included.
    assert!(header.contains(" scale=0.35 tokens="), "{header}");
    // The stored text has no `at`, so its id depends on the page alone.
    let stored = String::from_utf8(blob(&client, &first.packet, scratch.path())).unwrap();
    assert_eq!(stored, first.text.replacen(&at, "", 1));
    assert_eq!(fs::read_to_string(first.dir.join("packet.txt")).unwrap(), first.text);
    let image = &first.images[0];
    assert_eq!(image.name, "c1.png");
    let png = fs::read(first.dir.join("c1.png")).unwrap();
    assert_eq!(blob(&client, &image.id, scratch.path()), png);
    daemon.stop();
}

#[test]
fn a_read_after_a_point_holds_only_the_ink_drawn_since() {
    let daemon = Daemon::start();
    let client = daemon.client();
    let scratch = TempDir::new().unwrap();
    client.canvas_create("idea").unwrap();
    draw(&client, 0);
    let first = read(&client, None);
    draw(&client, 30_000);
    let second = read(&client, Some(1));
    assert_eq!(section(&second.text, "new"), ["c2"]);
    assert_eq!(section(&second.text, "page"), ["c1"]);
    assert!(second.text.contains(" from=2 to=2 "));
    assert_ne!(second.packet, first.packet);
    let stored = String::from_utf8(blob(&client, &second.packet, scratch.path())).unwrap();
    assert!(stored.starts_with("slinger-packet 1 canvas=idea "), "{stored}");
    // A read stores nothing new when nothing changed.
    assert_eq!(read(&client, Some(1)).packet, second.packet);
    daemon.stop();
}

#[test]
fn without_a_point_the_last_sling_on_the_canvas_is_the_marker() {
    let daemon = Daemon::start();
    let client = daemon.client();
    client.canvas_create("idea").unwrap();
    draw(&client, 0);
    let slung = read(&client, None);
    // A plain read moves nothing: the same ink is still new.
    assert_eq!(section(&read(&client, None).text, "new"), ["c1"]);
    let tags = NewTags { canvas: Some("idea".into()), ..NewTags::default() };
    let payload = json!({ "prompt": "look", "packet": slung.packet });
    client
        .record_append(slinger_control::NewEvent { tags, ..new_event("sling_sent", payload) })
        .unwrap();
    // A sling on another canvas is not this one's marker.
    let other = json!({ "prompt": "x", "packet": slung.packet });
    let tags = NewTags { canvas: Some("other".into()), ..NewTags::default() };
    client
        .record_append(slinger_control::NewEvent { tags, ..new_event("sling_sent", other) })
        .unwrap();

    let nothing = read(&client, None);
    assert!(nothing.text.contains(" from=2 to=1 "), "{}", nothing.text);
    assert!(section(&nothing.text, "new").is_empty() && nothing.images.is_empty());
    draw(&client, 30_000);
    assert_eq!(section(&read(&client, None).text, "new"), ["c2"]);
    assert_eq!(section(&read(&client, Some(0)).text, "new"), ["c1", "c2"]);
    daemon.stop();
}

#[test]
fn out_page_and_select_shape_the_read() {
    let daemon = Daemon::start();
    let client = daemon.client();
    client.canvas_create("idea").unwrap();
    draw(&client, 0);
    draw(&client, 30_000);
    let out = TempDir::new().unwrap();
    let ask = PacketAsk {
        canvas: "idea".into(),
        page: Some("p1".into()),
        after: Some(2),
        select: vec!["c2".into()],
        out: Some(out.path().join("here")),
    };
    let packet = client.canvas_read(ask).unwrap();
    assert_eq!(packet.dir, out.path().join("here"));
    assert_eq!(section(&packet.text, "page"), ["c2"]);
    assert!(packet.dir.join("packet.txt").exists());
    daemon.stop();
}

#[test]
fn a_bad_read_gets_its_code_and_a_hint() {
    let daemon = Daemon::start();
    let client = daemon.client();
    let ask = |canvas: &str| PacketAsk { canvas: canvas.into(), ..PacketAsk::default() };
    assert_eq!(client.canvas_read(ask("gone")).unwrap_err().code(), "not_found");
    client.canvas_create("idea").unwrap();
    let page = PacketAsk { page: Some("nope".into()), ..ask("idea") };
    assert_eq!(client.canvas_read(page).unwrap_err().code(), "not_found");
    let select = PacketAsk { select: vec!["zz".into()], ..ask("idea") };
    let err = client.canvas_read(select).unwrap_err();
    assert_eq!(err.code(), "invalid");
    assert!(err.hint().is_some());

    // A sling that names a packet the record does not hold.
    let missing = format!("sha256:{}", "0".repeat(64));
    let tags = NewTags { canvas: Some("idea".into()), ..NewTags::default() };
    let payload = json!({ "prompt": "x", "packet": missing });
    client
        .record_append(slinger_control::NewEvent { tags, ..new_event("sling_sent", payload) })
        .unwrap();
    let err = client.canvas_read(ask("idea")).unwrap_err();
    assert_eq!(err.code(), "invalid");
    assert!(err.hint().unwrap().contains("--after"));
    assert!(client.canvas_read(PacketAsk { after: Some(0), ..ask("idea") }).is_ok());
    daemon.stop();
}

#[test]
fn a_raw_request_reads() {
    let daemon = Daemon::start();
    daemon.client().canvas_create("idea").unwrap();
    let line = json!({ "v": 1, "op": "canvas_read", "canvas": "idea" });
    let reply: Value = serde_json::from_str(&daemon.raw(format!("{line}\n").as_bytes())).unwrap();
    assert_eq!(reply["ok"], true);
    assert!(reply["packet"].as_str().unwrap().starts_with("sha256:"));
    assert!(reply["text"].as_str().unwrap().starts_with("slinger-packet 1 "));
    daemon.stop();
}
