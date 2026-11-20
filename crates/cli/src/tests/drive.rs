//! `slinger drive` against a stand-in for the driven app: a socket that
//! takes one request line and answers it.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixListener;
use std::path::Path;

use serde_json::{Value, json};
use tempfile::TempDir;

use super::Run;
use crate::cli::run;
use crate::env::Env;
use crate::output::exit;

/// Runs `slinger drive` with `args` and the app's socket in the environment.
fn drive(app: Option<&Path>, args: &[&str]) -> Run {
    let mut line = vec!["slinger", "drive"];
    line.extend(args);
    let env = Env { drive: app.map(Path::to_path_buf), ..Env::default() };
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let code = run(line, &env, &mut std::io::empty(), &mut out, &mut err, false);
    let text = |bytes: Vec<u8>| String::from_utf8(bytes).unwrap();
    Run { code, out: text(out), err: text(err) }
}

/// Runs `slinger drive` with `args` against an app that answers `reply`:
/// the run, and the request the app got.
fn asked(args: &[&str], reply: &Value) -> (Run, Value) {
    let dir = TempDir::new().unwrap();
    let socket = dir.path().join("a.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let reply = reply.to_string();
    let app = std::thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut line = String::new();
        BufReader::new(&stream).read_line(&mut line).unwrap();
        writeln!(&stream, "{reply}").unwrap();
        serde_json::from_str::<Value>(&line).unwrap()
    });
    let run = drive(Some(&socket), args);
    (run, app.join().unwrap())
}

#[test]
fn drive_sends_one_request_and_prints_what_the_window_said() {
    let taken = json!({ "ok": true, "said": "taken" });
    let (run, got) = asked(&["key", "cmd+j"], &taken);
    assert_eq!((run.code, run.out.as_str(), run.err.as_str()), (exit::OK, "taken\n", ""));
    assert_eq!(got, json!({ "op": "key", "chord": "cmd+j", "text": null }));

    let (_, got) = asked(&["stroke", "1,2", "3.5,4", "--tablet"], &taken);
    assert_eq!(got["points"], json!([[1.0, 2.0], [3.5, 4.0]]));
    assert_eq!((&got["tablet"], &got["shift"]), (&json!(true), &json!(false)));

    let (_, got) = asked(&["scroll", "10", "20", "0", "-30"], &taken);
    assert_eq!((&got["op"], &got["dy"]), (&json!("scroll"), &json!(-30.0)));

    let (_, got) = asked(&["space"], &taken);
    assert_eq!(got, json!({ "op": "space", "down": true }));
    let (_, got) = asked(&["space", "--up"], &taken);
    assert_eq!(got["down"], false);

    let (_, got) = asked(&["paste"], &taken);
    assert_eq!(got, json!({ "op": "paste" }), "the driven window's own clipboard");
    let (_, got) = asked(&["paste", "two", "words"], &taken);
    assert_eq!(got["text"], "two words");
    let (_, got) = asked(&["copy"], &taken);
    assert_eq!(got, json!({ "op": "copy" }));

    let (_, got) = asked(&["shot", "w.png"], &taken);
    assert!(Path::new(got["path"].as_str().unwrap()).is_absolute(), "{got}");
}

#[test]
fn drive_fails_loudly_with_no_app_a_refusal_or_a_bad_point() {
    let none = drive(None, &["tick"]);
    assert_eq!(none.code, exit::USAGE);
    assert!(none.err.contains("SLINGER_DRIVE"), "{}", none.err);

    let gone = drive(Some(Path::new("/nonexistent/a.sock")), &["tick"]);
    assert_eq!(gone.code, exit::UNAVAILABLE);
    assert!(gone.err.contains("slinger-app --drive"), "{}", gone.err);

    let refused = json!({ "ok": false, "error": "no chord \"hyper+x\"" });
    let (run, _) = asked(&["key", "hyper+x"], &refused);
    assert_eq!(run.code, exit::INVALID);
    assert!(run.err.contains("no chord"), "{}", run.err);

    let point = drive(Some(Path::new("/nonexistent/a.sock")), &["stroke", "1;2"]);
    assert_eq!(point.code, exit::USAGE);
}
