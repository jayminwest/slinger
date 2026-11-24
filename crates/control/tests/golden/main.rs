//! Golden lines for every verb: the request line each `Client` method
//! writes, and the reply line the daemon writes, byte for byte
//! (slinger-ff3484). They hold the wire still while the code that writes it
//! changes: any difference fails, and a new verb fails until it has its
//! lines here and its row in docs/service.md's verb table.
//!
//! - `fixtures/requests.jsonl`: one line per call in [`calls::CALLS`], in
//!   that order, exactly as the client sent it.
//! - `fixtures/replies.jsonl`: one line per verb in [`replies::REPLIES`],
//!   in that order, exactly as `write_reply` writes it. The fake daemon
//!   answers each request with its verb's line, so each client method is
//!   also shown to read its verb's reply.
//!
//! A fixture is never rewritten to make this pass: a line that differs is
//! a change to the wire (docs/service.md, "Versioning").

#![cfg(test)]

mod calls;
mod replies;

use std::collections::BTreeSet;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixListener;
use std::path::Path;
use std::sync::mpsc;

use serde_json::Value;
use slinger_control::{Client, OPS, read_request};

/// How many verbs the protocol has. A new verb raises it, with its lines.
const VERBS: usize = 59;

fn fixture(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn op(line: &str) -> String {
    let doc: Value = serde_json::from_str(line).unwrap_or_else(|e| panic!("{e}: {line}"));
    doc["op"].as_str().unwrap_or_else(|| panic!("no op: {line}")).to_string()
}

/// The reply line of each verb, by op.
fn reply_lines() -> Vec<(&'static str, String)> {
    let lines: Vec<String> = fixture("replies.jsonl").lines().map(str::to_string).collect();
    assert_eq!(lines.len(), replies::REPLIES.len(), "one reply line per entry of REPLIES");
    replies::REPLIES.iter().map(|(op, _)| *op).zip(lines).collect()
}

/// A fake daemon on a socket in `dir`: it sends each request line it reads
/// to the channel and answers with the reply line of the line's verb.
fn recording(dir: &Path) -> (Client, mpsc::Receiver<String>) {
    let socket = dir.join("d.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let replies = reply_lines();
    let (heard, lines) = mpsc::channel();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let mut stream = stream.unwrap();
            let mut line = String::new();
            BufReader::new(&stream).read_line(&mut line).unwrap();
            let asked = op(&line);
            let reply = replies.iter().find(|(op, _)| *op == asked).map_or_else(
                || format!("{{\"ok\":false,\"code\":\"invalid\",\"error\":\"no reply line for {asked}\"}}\n"),
                |(_, reply)| format!("{reply}\n"),
            );
            if heard.send(line).is_err() {
                return;
            }
            stream.write_all(reply.as_bytes()).unwrap();
        }
    });
    (Client::new(socket), lines)
}

#[test]
fn every_client_method_writes_its_golden_request_line() {
    let dir = tempfile::tempdir().unwrap();
    let (client, heard) = recording(dir.path());
    let want: Vec<String> = fixture("requests.jsonl").lines().map(str::to_string).collect();
    for (i, call) in calls::CALLS.iter().enumerate() {
        let result = call(&client);
        let line = heard.try_recv().unwrap_or_else(|_| panic!("call {i} sent no request"));
        let line = line.strip_suffix('\n').unwrap_or_else(|| panic!("no newline: {line}"));
        result.unwrap_or_else(|e| panic!("call {i} ({line}) failed: {e}"));
        let golden = want.get(i).unwrap_or_else(|| panic!("no golden line {i}; add:\n{line}"));
        assert_eq!(line, golden, "call {i}: the request line changed");
    }
    assert_eq!(want.len(), calls::CALLS.len(), "a golden line has no call");
}

#[test]
fn every_golden_request_line_is_a_request() {
    for line in fixture("requests.jsonl").lines() {
        let read = read_request(&mut format!("{line}\n").as_bytes());
        assert!(matches!(read, Ok(Some(_))), "{line}: {read:?}");
    }
}

#[test]
fn every_golden_reply_line_writes_back_byte_for_byte() {
    for ((op, round), (_, line)) in replies::REPLIES.iter().zip(reply_lines()) {
        assert_eq!(round(&line), format!("{line}\n"), "{op}: the reply line changed");
    }
}

#[test]
fn every_verb_has_its_lines_and_no_line_is_for_no_verb() {
    let requested: BTreeSet<String> = fixture("requests.jsonl").lines().map(op).collect();
    let replied: BTreeSet<String> = replies::REPLIES.iter().map(|(op, _)| op.to_string()).collect();
    assert_eq!(replied.len(), replies::REPLIES.len(), "a verb has two reply lines");
    assert_eq!(requested, replied, "verbs with a request line but no reply line, or the other way");
    let verbs: BTreeSet<String> = OPS.iter().copied().map(String::from).collect();
    assert_eq!(requested, verbs, "verbs with lines but no row in OPS, or the other way");
    assert_eq!((requested.len(), OPS.len()), (VERBS, VERBS));
}

/// The ops in docs/service.md's verb table: the first cell of each row
/// under the header row that starts with the op column, several to a cell.
fn documented() -> BTreeSet<String> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/service.md");
    let text = std::fs::read_to_string(&path).unwrap();
    let mut rows =
        text.lines().skip_while(|line| !line.starts_with("| `op` | Arguments | Result |"));
    assert!(rows.next().is_some(), "docs/service.md has no verb table");
    let ops: BTreeSet<String> = rows
        .skip(1)
        .take_while(|line| line.starts_with('|'))
        .flat_map(|row| {
            let cell = row.split('|').nth(1).unwrap_or_default();
            cell.split('`').skip(1).step_by(2).map(str::to_string).collect::<Vec<_>>()
        })
        .collect();
    ops
}

#[test]
fn the_service_md_verb_table_lists_exactly_the_verbs() {
    let documented = documented();
    let verbs: BTreeSet<String> = OPS.iter().copied().map(String::from).collect();
    let undocumented: Vec<_> = verbs.difference(&documented).collect();
    let unknown: Vec<_> = documented.difference(&verbs).collect();
    assert!(
        undocumented.is_empty(),
        "verbs missing from docs/service.md's table: {undocumented:?}"
    );
    assert!(unknown.is_empty(), "docs/service.md's table lists verbs the code lacks: {unknown:?}");
}
