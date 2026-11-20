//! The line `slinger drive` writes for each op, byte for byte, against the
//! golden lines in `crates/control/tests/fixtures/drive.jsonl` (the app
//! reads the same lines in crates/app/src/drive.rs's tests).
//! `SLINGER_BLESS=1` writes the file from what the CLI sends.

use std::io::{BufRead, BufReader, Write};
use std::path::Path;

use tempfile::TempDir;

use crate::cli::run;
use crate::env::Env;
use crate::output::exit;

/// One run per op the CLI sends, in the order of the fixture's lines.
const RUNS: &[&[&str]] = &[
    &["key", "cmd+j"],
    &["key", "escape", "--text", "\u{1b}"],
    &["type", "Hi", "there"],
    &["press", "10,20", "--tablet", "--shift"],
    &["drag", "10.5", "-2", "--button", "middle"],
    &["lift", "5,6", "--control", "--touch", "--eraser"],
    &["stroke", "1,2", "3.5,4", "--tablet"],
    &["move", "10,20"],
    &["scroll", "10,20", "0,-30"],
    &["zoom", "10", "20", "1.5"],
    &["space"],
    &["space", "--up"],
    &["run", "toggle-sidebar"],
    &["copy"],
    &["cut"],
    &["paste"],
    &["paste", "two", "words"],
    &["clip"],
    &["pick", "New", "text"],
    &["drop", "10,20", "/tmp/a.pdf"],
    &["tick"],
    &["scene"],
    &["scene", "--debug"],
    &["shot", "/tmp/w.png"],
    &["resize", "800", "600.5"],
];

/// The line `slinger drive ARGS` writes to the app's socket.
fn sent(args: &[&str]) -> String {
    let dir = TempDir::new().unwrap();
    let socket = dir.path().join("a.sock");
    let listener = slinger_control::bind_socket(&socket).unwrap();
    let app = std::thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut line = String::new();
        BufReader::new(&stream).read_line(&mut line).unwrap();
        writeln!(&stream, r#"{{"ok":true,"said":""}}"#).unwrap();
        line
    });
    let mut line = vec!["slinger", "drive"];
    line.extend(args);
    let env = Env { drive: Some(socket.clone()), ..Env::default() };
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let code = run(line, &env, &mut std::io::empty(), &mut out, &mut err, false);
    assert_eq!(code, exit::OK, "{args:?}: {}", String::from_utf8_lossy(&err));
    app.join().unwrap()
}

#[test]
fn each_drive_op_writes_its_golden_line() {
    let fixture =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../control/tests/fixtures/drive.jsonl");
    let lines: String = RUNS.iter().map(|args| sent(args)).collect();
    if std::env::var_os("SLINGER_BLESS").is_some() {
        std::fs::write(&fixture, &lines).unwrap();
    }
    let golden = std::fs::read_to_string(&fixture).unwrap();
    for ((args, got), want) in RUNS.iter().zip(lines.lines()).zip(golden.lines()) {
        assert_eq!(got, want, "slinger drive {}", args.join(" "));
    }
    assert_eq!(lines.lines().count(), golden.lines().count(), "a run per golden line");
}
