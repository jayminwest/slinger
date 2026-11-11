//! Flick's dictation (SPEC D2, K6): slinger has no speech code. Flick, the
//! owner's OS layer, records and transcribes on this Mac and types the text
//! into the focused field. The app only asks it to start, stop or cancel,
//! over Flick's control socket, as the `flick` CLI does: one line, a JSON
//! array of strings (`["dictation","start"]`), answered by one line,
//! `{"ok":"<text>"}` or `{"error":"<message>"}` (Flick's docs/cli.md). The
//! socket is `$FLICK_SOCKET`, else Flick's default path. A thread of its own
//! asks, so the main thread never waits on Flick.

use std::io::{BufRead, BufReader, ErrorKind, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::Duration;

use crate::backend::Problem;
use crate::scene::dictation::Verb;

/// How long Flick has to answer one request. Its dictation verbs answer at
/// once: the recording and the transcription run on its own threads.
const TIMEOUT: Duration = Duration::from_secs(3);

/// What to do when Flick does not answer.
const NOT_RUNNING_HINT: &str = "start Flick to dictate, or type the prompt";
/// What to do when Flick refuses.
const REFUSED_HINT: &str = "'flick dictation status' says what is missing";

/// The thread that asks Flick. Dropping it ends the thread once the request
/// in flight is answered.
#[derive(Debug)]
pub struct Flick {
    asks: Sender<Verb>,
    answers: Receiver<(Verb, Result<(), Problem>)>,
}

impl Flick {
    /// Starts asking the Flick listening on `socket`. Returns at once; no
    /// request goes until [`Flick::send`].
    pub fn start(socket: PathBuf) -> Self {
        let (asks, to_send) = mpsc::channel::<Verb>();
        let (answer, answers) = mpsc::channel();
        thread::spawn(move || {
            for verb in to_send {
                let said = ask(&socket, &["dictation", verb.word()]).map(drop);
                if answer.send((verb, said)).is_err() {
                    return;
                }
            }
        });
        Self { asks, answers }
    }

    /// Flick's control socket: `$FLICK_SOCKET`, else
    /// `~/Library/Application Support/Flick/flick.sock`.
    pub fn socket() -> PathBuf {
        if let Some(path) = std::env::var_os("FLICK_SOCKET") {
            return PathBuf::from(path);
        }
        let home = PathBuf::from(std::env::var_os("HOME").unwrap_or_default());
        home.join("Library/Application Support/Flick/flick.sock")
    }

    /// Hands `verb` to the thread. Never waits.
    pub fn send(&self, verb: Verb) {
        // The thread lives as long as the handle, so the send cannot fail.
        let _ = self.asks.send(verb);
    }

    /// Flick's answers since the last call, oldest first. Never waits.
    pub fn answers(&self) -> Vec<(Verb, Result<(), Problem>)> {
        self.answers.try_iter().collect()
    }
}

/// Sends `words` as one request to the Flick on `socket` and returns its
/// `ok` text. A Flick that cannot be reached is not running; its `error`
/// reply is the problem, as it says it.
fn ask(socket: &Path, words: &[&str]) -> Result<String, Problem> {
    let not_running = |message: String| Problem::new(message, Some(NOT_RUNNING_HINT));
    let mut stream = UnixStream::connect(socket).map_err(|e| match e.kind() {
        ErrorKind::NotFound | ErrorKind::ConnectionRefused => {
            not_running("Flick is not running".to_string())
        }
        _ => not_running(format!("Flick: {e}")),
    })?;
    let line = serde_json::to_string(words).unwrap_or_default();
    let mut reply = String::new();
    stream
        .set_read_timeout(Some(TIMEOUT))
        .and_then(|()| stream.set_write_timeout(Some(TIMEOUT)))
        .and_then(|()| writeln!(stream, "{line}"))
        .and_then(|()| BufReader::new(&stream).read_line(&mut reply))
        .map_err(|e| not_running(format!("Flick did not answer: {e}")))?;
    let reply: serde_json::Value = serde_json::from_str(&reply)
        .map_err(|_| not_running("Flick did not answer".to_string()))?;
    if let Some(error) = reply.get("error").and_then(serde_json::Value::as_str) {
        return Err(Problem::new(error.to_string(), Some(REFUSED_HINT)));
    }
    match reply.get("ok") {
        Some(serde_json::Value::String(text)) => Ok(text.clone()),
        Some(other) => Ok(other.to_string()),
        None => Err(not_running("Flick did not answer".to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::testing::{fake_flick, ready, wait_until};

    fn off(_line: &str) -> String {
        r#"{"error":"dictation: off: set a key in [dictation] to turn it on"}"#.to_string()
    }

    fn next(flick: &Flick) -> (Verb, Result<(), Problem>) {
        let mut got = None;
        wait_until(|| {
            got = flick.answers().pop();
            got.is_some()
        });
        got.unwrap()
    }

    #[test]
    fn each_verb_is_one_request_line_as_the_flick_cli_sends_it() {
        let dir = tempfile::tempdir().unwrap();
        let (socket, heard) = fake_flick(dir.path(), ready);
        let flick = Flick::start(socket);
        for verb in [Verb::Start, Verb::Stop, Verb::Cancel] {
            flick.send(verb);
            assert_eq!(next(&flick), (verb, Ok(())));
        }
        let heard = heard.lock().unwrap().clone();
        let want =
            [r#"["dictation","start"]"#, r#"["dictation","stop"]"#, r#"["dictation","cancel"]"#];
        assert_eq!(heard, want);
    }

    #[test]
    fn a_refusal_is_flick_s_own_message_with_where_to_look() {
        let dir = tempfile::tempdir().unwrap();
        let (socket, _) = fake_flick(dir.path(), off);
        let flick = Flick::start(socket);
        flick.send(Verb::Start);
        let (verb, said) = next(&flick);
        let problem = said.unwrap_err();
        assert_eq!(verb, Verb::Start);
        assert!(problem.message.starts_with("dictation: off"), "{problem:?}");
        assert_eq!(problem.hint.as_deref(), Some(REFUSED_HINT));
    }

    #[test]
    fn with_no_flick_listening_it_says_flick_is_not_running() {
        let dir = tempfile::tempdir().unwrap();
        let flick = Flick::start(dir.path().join("flick.sock"));
        flick.send(Verb::Start);
        let problem = next(&flick).1.unwrap_err();
        assert_eq!(problem.message, "Flick is not running");
        assert_eq!(problem.hint.as_deref(), Some(NOT_RUNNING_HINT));
    }

    #[test]
    fn a_reply_that_is_not_flick_s_is_no_answer() {
        let dir = tempfile::tempdir().unwrap();
        let (socket, _) = fake_flick(dir.path(), |_| "hello".to_string());
        let problem = ask(&socket, &["dictation", "start"]).unwrap_err();
        assert_eq!(problem.message, "Flick did not answer");
        let other = tempfile::tempdir().unwrap();
        let (socket, _) = fake_flick(other.path(), |_| "{}".to_string());
        assert!(ask(&socket, &["dictation", "status"]).is_err(), "neither ok nor error");
    }

    #[test]
    fn the_socket_is_flick_s_default_under_home() {
        // Read only: tests never set the process's environment.
        if std::env::var_os("FLICK_SOCKET").is_none() {
            assert!(Flick::socket().ends_with("Library/Application Support/Flick/flick.sock"));
        }
    }
}
