//! The CLI driven in-process through `cli::run`: real argv in, bytes and an
//! exit code out. Each run talks to a real daemon on its own socket.
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

use serde_json::Value;

use slingerd as daemon;

use crate::cli::{NAME, run};
use crate::output::{Ctx, color_enabled, exit};

struct Run {
    code: u8,
    out: String,
    err: String,
}

/// A socket path that no other test uses. Short: socket paths have a length limit.
fn socket_path() -> PathBuf {
    static NEXT: AtomicU32 = AtomicU32::new(0);
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("c-{}-{n}.sock", std::process::id()))
}

/// Runs the CLI against a daemon that lives for this one call.
fn cli(args: &[&str]) -> Run {
    let socket = socket_path();
    let server = daemon::Server::bind(&socket).unwrap();
    let stopper = server.stopper();
    let serving = std::thread::spawn(move || server.serve());
    let run = cli_on(&socket, args);
    stopper.stop();
    serving.join().unwrap().unwrap();
    std::fs::remove_file(format!("{}.lock", socket.display())).unwrap();
    run
}

fn cli_on(socket: &std::path::Path, args: &[&str]) -> Run {
    let socket = socket.to_str().unwrap();
    let args: Vec<&str> = args.iter().copied().chain(["--socket", socket]).collect();
    cli_bare(&args)
}

fn cli_bare(args: &[&str]) -> Run {
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let argv0_and_args = std::iter::once(NAME).chain(args.iter().copied());
    let code = run(argv0_and_args, &mut out, &mut err, false);
    Run {
        code,
        out: String::from_utf8_lossy(&out).into(),
        err: String::from_utf8_lossy(&err).into(),
    }
}

fn json(r: &Run) -> Value {
    serde_json::from_str(&r.out).unwrap_or_else(|e| panic!("stdout is not JSON ({e}): {}", r.out))
}

#[test]
fn hello_prints_the_greeting_and_nothing_else() {
    let r = cli(&["hello", "ada"]);
    assert_eq!((r.code, r.out.as_str(), r.err.as_str()), (exit::OK, "hello, ada\n", ""));
}

#[test]
fn hello_json_is_one_envelope() {
    let doc = json(&cli(&["hello", "ada", "--json"]));
    assert_eq!(doc["success"], true);
    assert_eq!(doc["command"], "hello");
    assert_eq!(doc["greeting"], "hello, ada");
}

#[test]
fn invalid_input_maps_to_its_exit_code_and_hint() {
    let r = cli(&["hello", " "]);
    assert_eq!(r.code, exit::INVALID);
    assert_eq!(r.out, "");
    assert!(r.err.starts_with("✗ name is empty\n  "), "{}", r.err);
}

#[test]
fn json_errors_go_to_stdout_with_a_code() {
    let r = cli(&["hello", " ", "--json"]);
    let doc = json(&r);
    assert_eq!(r.code, exit::INVALID);
    assert_eq!(doc["success"], false);
    assert_eq!(doc["code"], "invalid");
    assert!(doc["hint"].is_string());
    assert_eq!(r.err, "");
}

#[test]
fn no_daemon_is_unavailable_with_a_start_hint() {
    let socket = socket_path();
    let r = cli_on(&socket, &["hello", "ada"]);
    assert_eq!((r.code, r.out.as_str()), (exit::UNAVAILABLE, ""));
    assert!(r.err.contains(" does not answer on "), "{}", r.err);
    assert!(r.err.contains("\n  start it with '"), "{}", r.err);
    let doc = json(&cli_on(&socket, &["hello", "ada", "--json"]));
    assert_eq!(doc["code"], "unavailable");
}

#[test]
fn version_and_help_need_no_daemon() {
    assert_eq!(cli_bare(&["--version"]).code, exit::OK);
    assert_eq!(cli_bare(&["hello", "--help"]).code, exit::OK);
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
        assert!(r.out.contains("hello"), "{}", r.out);
    }
}

#[test]
fn verbose_writes_diagnostics_to_stderr_only() {
    let r = cli(&["--verbose", "hello", "ada"]);
    assert_eq!(r.out, "hello, ada\n");
    assert!(r.err.contains("greeting"), "{}", r.err);
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
