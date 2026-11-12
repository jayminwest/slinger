//! The CLI driven in-process through `cli::run`: real argv in, bytes and an
//! exit code out. Each run talks to a real daemon on its own socket.
use std::io::Write;
use std::path::{Path, PathBuf};

use serde_json::Value;
use tempfile::TempDir;

use slingerd as daemon;

use crate::cli::{NAME, run};
use crate::output::{Ctx, color_enabled, exit};

mod canvas;
mod canvas_feed;
mod edit;
mod feedback;
mod look;
mod pane;
mod put;
mod read;
mod session;
mod sling;
mod ui_open;
mod workspace;

struct Run {
    code: u8,
    out: String,
    err: String,
}

/// A socket path in a new directory of its own, so no other test, process or
/// leftover from an earlier run shares it, and it is removed on drop even
/// when the test fails. Short: socket paths have a length limit.
fn socket_path() -> (TempDir, PathBuf) {
    let dir = TempDir::new().unwrap();
    let socket = dir.path().join("d.sock");
    (dir, socket)
}

/// Runs the CLI against a daemon that lives for this one call.
fn cli(args: &[&str]) -> Run {
    cli_each(&[args]).remove(0)
}

/// Runs the CLI once per entry, in order, against one daemon with an empty record.
fn cli_each(runs: &[&[&str]]) -> Vec<Run> {
    with_daemon(|socket| runs.iter().map(|args| cli_on(socket, args)).collect())
}

/// Runs `f` against a daemon with an empty record that lives for this one
/// call. An agent's process cannot start: its program is not there, so no
/// test runs the real Claude Code by accident.
fn with_daemon<T>(f: impl FnOnce(&Path) -> T) -> T {
    with_daemon_running(Path::new("/nonexistent/claude"), f)
}

/// [`with_daemon`], with each agent's process running `claude`.
fn with_daemon_running<T>(claude: &Path, f: impl FnOnce(&Path) -> T) -> T {
    let (_dir, socket) = socket_path();
    let data = tempfile::tempdir().unwrap();
    let store = daemon::Store::open(data.path(), "my-laptop").unwrap().with_claude(claude);
    let server = daemon::Server::bind(&socket).unwrap();
    let stopper = server.stopper();
    let serving = std::thread::spawn(move || server.serve(&store));
    let result = f(&socket);
    stopper.stop();
    serving.join().unwrap().unwrap();
    result
}

fn cli_on(socket: &Path, args: &[&str]) -> Run {
    cli_raw(socket, args, b"", &mut Vec::new())
}

/// Runs the CLI with `input` on stdin and `out` as stdout, keeping a copy of stdout.
fn cli_raw(socket: &Path, args: &[&str], input: &[u8], out: &mut dyn Write) -> Run {
    let socket = socket.to_str().unwrap();
    let args: Vec<&str> = args.iter().copied().chain(["--socket", socket]).collect();
    let mut copy = Vec::new();
    let mut run = cli_with(&args, input, &mut Tee(out, &mut copy));
    run.out = String::from_utf8_lossy(&copy).into();
    run
}

/// Writes to the caller's stdout and keeps a copy.
struct Tee<'a>(&'a mut dyn Write, &'a mut Vec<u8>);

impl Write for Tee<'_> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let n = self.0.write(buf)?;
        self.1.extend_from_slice(&buf[..n]);
        Ok(n)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.0.flush()
    }
}

fn cli_bare(args: &[&str]) -> Run {
    let mut out = Vec::new();
    let mut run = cli_with(args, b"", &mut out);
    run.out = String::from_utf8_lossy(&out).into();
    run
}

/// Runs the CLI; `out` gets stdout, which the returned run leaves empty.
fn cli_with(args: &[&str], input: &[u8], out: &mut dyn Write) -> Run {
    let mut err = Vec::new();
    let argv0_and_args = std::iter::once(NAME).chain(args.iter().copied());
    let code = run(argv0_and_args, &mut &input[..], out, &mut err, false);
    Run { code, out: String::new(), err: String::from_utf8_lossy(&err).into() }
}

fn json(r: &Run) -> Value {
    serde_json::from_str(&r.out).unwrap_or_else(|e| panic!("stdout is not JSON ({e}): {}", r.out))
}

#[test]
fn success_prints_the_result_and_nothing_else() {
    let r = cli(&["canvas", "list"]);
    assert_eq!((r.code, r.out.as_str(), r.err.as_str()), (exit::OK, "", ""));
}

#[test]
fn json_success_is_one_envelope() {
    let doc = json(&cli(&["canvas", "list", "--json"]));
    assert_eq!(doc["success"], true);
    assert_eq!(doc["command"], "canvas list");
    assert_eq!(doc["canvases"], serde_json::json!([]));
}

#[test]
fn invalid_input_maps_to_its_exit_code_and_hint() {
    let r = cli(&["canvas", "create", "Main"]);
    assert_eq!(r.code, exit::INVALID);
    assert_eq!(r.out, "");
    assert!(r.err.starts_with("✗ \"Main\" is not a canvas id\n  "), "{}", r.err);
}

#[test]
fn json_errors_go_to_stdout_with_a_code() {
    let r = cli(&["canvas", "create", "Main", "--json"]);
    let doc = json(&r);
    assert_eq!(r.code, exit::INVALID);
    assert_eq!(doc["success"], false);
    assert_eq!(doc["code"], "invalid");
    assert!(doc["hint"].is_string());
    assert_eq!(r.err, "");
}

#[test]
fn no_daemon_is_unavailable_with_a_start_hint() {
    let (_dir, socket) = socket_path();
    let r = cli_on(&socket, &["canvas", "list"]);
    assert_eq!((r.code, r.out.as_str()), (exit::UNAVAILABLE, ""));
    assert!(r.err.contains(" does not answer on "), "{}", r.err);
    assert!(r.err.contains("\n  start it with '"), "{}", r.err);
    let doc = json(&cli_on(&socket, &["canvas", "list", "--json"]));
    assert_eq!(doc["code"], "unavailable");
}

#[test]
fn record_append_prints_the_number_and_list_prints_one_line_per_event() {
    let runs = cli_each(&[
        &["record", "append", "agent_done", r#"{"summary":"wrote it"}"#, "--agent", "scout"],
        &["record", "append", "agent_blocked", r#"{"reason":"which?"}"#, "--workspace", "notes"],
        &["record", "list"],
        &["record", "list", "--kind", "agent_blocked"],
        &["record", "list", "--after", "1", "--workspace", "notes", "--machine", "my-laptop"],
        &["record", "list", "--agent", "nobody", "--canvas", "c1"],
    ]);
    let outs: Vec<(u8, &str, &str)> =
        runs.iter().map(|r| (r.code, r.out.as_str(), r.err.as_str())).collect();
    let done = "1\tagent_done\tmachine=my-laptop agent=scout\t{\"summary\":\"wrote it\"}\n";
    let blocked = "2\tagent_blocked\tmachine=my-laptop workspace=notes\t{\"reason\":\"which?\"}\n";
    assert_eq!(outs[..2], [(exit::OK, "1\n", ""), (exit::OK, "2\n", "")]);
    assert_eq!(outs[2], (exit::OK, format!("{done}{blocked}").as_str(), ""));
    assert_eq!(outs[3..], [(exit::OK, blocked, ""), (exit::OK, blocked, ""), (exit::OK, "", "")]);
}

#[test]
fn record_json_is_one_envelope_and_an_empty_list_succeeds() {
    let runs = cli_each(&[
        &["record", "list", "--json"],
        &["record", "append", "agent_done", "--source", "scout", "--canvas", "c1", "--json"],
        &["record", "list", "--json"],
    ]);
    let docs: Vec<Value> = runs.iter().map(json).collect();
    assert_eq!(runs.iter().map(|r| r.code).collect::<Vec<_>>(), [exit::OK; 3]);
    assert_eq!(docs[0]["command"], "record list");
    assert_eq!(docs[0]["events"], serde_json::json!([]));
    assert_eq!(docs[1]["command"], "record append");
    let event = &docs[1]["event"];
    assert_eq!((event["number"].as_u64(), event["source"].as_str()), (Some(1), Some("scout")));
    assert_eq!(event["tags"], serde_json::json!({"machine": "my-laptop", "canvas": "c1"}));
    assert_eq!(event["payload"], serde_json::json!({}));
    assert_eq!(docs[2]["events"], serde_json::json!([event]));
}

#[test]
fn an_invalid_payload_or_kind_is_invalid_with_a_hint() {
    for args in [
        &["record", "append", "agent_blocked", "{}"][..],
        &["record", "append", "agent_done", "not json"],
        &["record", "append", "no_such_kind"],
    ] {
        let r = cli(args);
        assert_eq!((r.code, r.out.as_str()), (exit::INVALID, ""), "{args:?}");
        assert!(r.err.starts_with("✗ ") && r.err.contains("\n  "), "{}", r.err);
        let mut args = args.to_vec();
        args.push("--json");
        let doc = json(&cli(&args));
        assert_eq!(
            (doc["success"].as_bool(), doc["code"].as_str()),
            (Some(false), Some("invalid"))
        );
        assert!(doc["hint"].is_string());
    }
}

const ABC: &str = "sha256:ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";

#[test]
fn blob_put_prints_the_id_and_get_writes_the_bytes_back() {
    let dir = tempfile::tempdir().unwrap();
    let (file, copy) = (dir.path().join("abc"), dir.path().join("copy"));
    std::fs::write(&file, "abc").unwrap();
    let (file, copy) = (file.to_str().unwrap(), copy.to_str().unwrap());
    let runs = cli_each(&[
        &["blob", "put", file],
        &["blob", "put", file],
        &["blob", "get", ABC],
        &["blob", "get", ABC, "--out", copy],
    ]);
    let outs: Vec<(u8, &str, &str)> =
        runs.iter().map(|r| (r.code, r.out.as_str(), r.err.as_str())).collect();
    let id = format!("{ABC}\n");
    assert_eq!(
        outs,
        [
            (exit::OK, id.as_str(), ""),
            (exit::OK, &id, ""),
            (exit::OK, "abc", ""),
            (exit::OK, "", "")
        ]
    );
    assert_eq!(std::fs::read_to_string(copy).unwrap(), "abc");
}

#[test]
fn blob_put_dash_reads_stdin_and_get_streams_any_bytes() {
    let bytes: Vec<u8> = (0..=255).cycle().take(3 * 1024 * 1024 + 7).collect();
    let (put, got) = with_daemon(|socket| {
        let put = cli_raw(socket, &["blob", "put", "-"], &bytes, &mut Vec::new());
        let mut got = Vec::new();
        let get = cli_raw(socket, &["blob", "get", put.out.trim()], b"", &mut got);
        assert_eq!(get.code, exit::OK);
        (put, got)
    });
    assert_eq!((put.code, put.err.as_str()), (exit::OK, ""));
    assert!(put.out.starts_with("sha256:"), "{}", put.out);
    assert!(got.iter().eq(&bytes), "the bytes differ");
}

#[test]
fn blob_json_is_one_envelope_and_json_with_stdout_bytes_is_a_usage_error() {
    let dir = tempfile::tempdir().unwrap();
    let (file, copy) = (dir.path().join("abc"), dir.path().join("copy"));
    std::fs::write(&file, "abc").unwrap();
    let runs = cli_each(&[
        &["blob", "put", file.to_str().unwrap(), "--json"],
        &["blob", "get", ABC, "--out", copy.to_str().unwrap(), "--json"],
        &["blob", "get", ABC, "--json"],
    ]);
    let docs: Vec<Value> = runs.iter().map(json).collect();
    assert_eq!(runs.iter().map(|r| r.code).collect::<Vec<_>>(), [exit::OK, exit::OK, exit::USAGE]);
    assert_eq!(
        (docs[0]["command"].as_str(), docs[0]["id"].as_str()),
        (Some("blob put"), Some(ABC))
    );
    assert_eq!((docs[1]["id"].as_str(), docs[1]["size"].as_u64()), (Some(ABC), Some(3)));
    assert_eq!((docs[2]["code"].as_str(), docs[2]["hint"].is_string()), (Some("usage"), true));
}

#[test]
fn an_unknown_blob_is_not_found_and_a_bad_id_is_invalid() {
    let runs = cli_each(&[
        &["blob", "get", ABC],
        &["blob", "get", "nope"],
        &["blob", "put", "/no/such/file"],
    ]);
    let codes: Vec<u8> = runs.iter().map(|r| r.code).collect();
    assert_eq!(codes, [exit::NOT_FOUND, exit::INVALID, exit::NOT_FOUND]);
    for r in &runs {
        assert!(r.out.is_empty() && r.err.starts_with("✗ ") && r.err.contains("\n  "), "{}", r.err);
    }
}

#[test]
fn a_closed_stdout_ends_a_blob_get_with_success() {
    struct Closed;
    impl Write for Closed {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(std::io::ErrorKind::BrokenPipe.into())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let code = with_daemon(|socket| {
        cli_raw(socket, &["blob", "put", "-"], b"abc", &mut Vec::new());
        cli_raw(socket, &["blob", "get", ABC], b"", &mut Closed).code
    });
    assert_eq!(code, exit::OK);
}

#[test]
fn ui_put_list_show_and_export() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("a.html");
    std::fs::write(&file, "<p>a</p>").unwrap();
    let (file, out) = (file.to_str().unwrap(), dir.path().join("out"));
    let runs = with_daemon(|socket| {
        let mut runs = vec![
            cli_on(socket, &["ui", "put", "pick", file]),
            cli_raw(socket, &["ui", "put", "pick", "-"], b"<p>b</p>", &mut Vec::new()),
            cli_on(socket, &["ui", "put", "pick", file, "--source", "scout", "--json"]),
        ];
        for args in [
            &["ui", "list"][..],
            &["ui", "show", "pick"],
            &["ui", "export", "pick@1", "--out", out.to_str().unwrap()],
            &["ui", "show", "nope"],
        ] {
            runs.push(cli_on(socket, args));
        }
        runs
    });
    let outs: Vec<(u8, &str)> = runs.iter().map(|r| (r.code, r.out.as_str())).collect();
    assert_eq!(outs[..2], [(exit::OK, "pick@1\n"), (exit::OK, "pick@2\n")]);
    let put = json(&runs[2]);
    assert_eq!((put["command"].as_str(), put["new"].as_bool()), (Some("ui put"), Some(true)));
    assert_eq!(
        (put["ui"]["version"].as_u64(), put["ui"]["source"].as_str()),
        (Some(3), Some("scout"))
    );
    let list: Vec<&str> = runs[3].out.lines().collect();
    assert_eq!(list.len(), 1);
    assert!(
        list[0].starts_with("pick@3\tsha256:") && list[0].contains("\t8\tscout\t"),
        "{}",
        list[0]
    );
    let show: Vec<&str> = runs[4].out.lines().map(|l| l.split('\t').next().unwrap()).collect();
    assert_eq!(show, ["pick@1", "pick@2", "pick@3"]);
    let exported = out.join("pick.v1.html");
    assert_eq!(runs[5].out, format!("{}\n", exported.display()));
    assert_eq!(std::fs::read_to_string(exported).unwrap(), "<p>a</p>");
    assert_eq!(runs[6].code, exit::NOT_FOUND);
    assert!(runs[6].err.contains("\n  list them with"), "{}", runs[6].err);
}

#[test]
fn version_and_help_need_no_daemon() {
    assert_eq!(cli_bare(&["--version"]).code, exit::OK);
    assert_eq!(cli_bare(&["canvas", "list", "--help"]).code, exit::OK);
}

#[test]
fn unknown_command_is_a_usage_error() {
    let r = cli(&["nope"]);
    assert_eq!(r.code, exit::USAGE);
    assert!(r.err.starts_with("✗ unrecognized subcommand"), "{}", r.err);
}

#[test]
fn usage_errors_honor_json() {
    let r = cli(&["--json", "--bogus"]);
    let doc = json(&r);
    assert_eq!(r.code, exit::USAGE);
    assert_eq!(doc["code"], "usage");
}

#[test]
fn version_is_bare_semver() {
    for flag in ["-v", "--version"] {
        let r = cli(&[flag]);
        assert_eq!((r.code, r.out.trim()), (exit::OK, env!("CARGO_PKG_VERSION")));
    }
}

#[test]
fn version_json_has_metadata() {
    let doc = json(&cli(&["--version", "--json"]));
    assert_eq!(doc["name"], NAME);
    assert_eq!(doc["version"], env!("CARGO_PKG_VERSION"));
    assert!(doc["platform"].is_string());
}

#[test]
fn no_args_and_help_print_help_and_succeed() {
    for args in [&[][..], &["--help"], &["-h"]] {
        let r = cli(args);
        assert_eq!(r.code, exit::OK);
        assert!(r.out.contains("Usage:"), "{}", r.out);
        assert!(r.out.contains("canvas"), "{}", r.out);
    }
}

#[test]
fn verbose_writes_diagnostics_to_stderr_only() {
    let r = cli(&["--verbose", "canvas", "list"]);
    assert_eq!((r.code, r.out.as_str()), (exit::OK, ""));
    assert!(r.err.starts_with("canvas list on "), "{}", r.err);
}

#[test]
fn color_follows_no_color_then_force_color_then_tty() {
    assert!(!color_enabled(true, Some("1"), true));
    assert!(color_enabled(false, Some("1"), false));
    assert!(!color_enabled(false, Some("0"), true));
    assert!(color_enabled(false, None, true));
    assert!(!color_enabled(false, None, false));
}

#[test]
fn info_is_silenced_by_quiet_and_json() {
    for (quiet, json, expected) in
        [(false, false, "working\n"), (true, false, ""), (false, true, "")]
    {
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let mut ctx = Ctx::new(&mut out, &mut err, json, false);
        ctx.quiet = quiet;
        ctx.info("working").unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(String::from_utf8_lossy(&err), expected);
    }
}
