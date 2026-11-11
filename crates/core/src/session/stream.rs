//! Claude Code's stream format (SPEC D10, Spike C): how a session's process is
//! started, what a message to it looks like, and what its output says.
//!
//! The process is `claude --print` with stream input and stream output: one
//! JSON object per line each way. It stays open between messages, so each
//! message lands in the same conversation. A session id names that
//! conversation; Slinger picks it for a fresh start and passes it back to
//! resume. No IO: the server spawns the process and moves the lines.

use std::fmt::Write;

use serde_json::{Value, json};

use crate::{Error, Result};

/// How a session's process starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Start<'a> {
    /// A new conversation under this id, which [`session_id`] made.
    Fresh(&'a str),
    /// The conversation this id names, as an earlier process left it.
    Resume(&'a str),
}

/// The arguments of the process, after the program's path.
#[must_use]
pub fn args(start: Start<'_>) -> Vec<String> {
    let (flag, id) = match start {
        Start::Fresh(id) => ("--session-id", id),
        Start::Resume(id) => ("--resume", id),
    };
    // Stream output in print mode needs --verbose.
    let fixed = ["--print", "--input-format", "stream-json", "--output-format", "stream-json"];
    fixed.into_iter().chain(["--verbose", flag, id]).map(str::to_string).collect()
}

/// A new session id, a UUID (version 4) made from `random`.
#[must_use]
pub fn session_id(mut random: [u8; 16]) -> String {
    random[6] = (random[6] & 0x0f) | 0x40;
    random[8] = (random[8] & 0x3f) | 0x80;
    let hex: String = random.iter().fold(String::new(), |mut hex, b| {
        let _ = write!(hex, "{b:02x}");
        hex
    });
    format!("{}-{}-{}-{}-{}", &hex[..8], &hex[8..12], &hex[12..16], &hex[16..20], &hex[20..])
}

/// The line that delivers `text` to the process as one user message, newline
/// included.
///
/// # Errors
///
/// [`Error::Invalid`] for blank text: there is nothing to deliver.
pub fn message(text: &str) -> Result<String> {
    if text.trim().is_empty() {
        return Err(Error::Invalid {
            message: "the message is empty".into(),
            hint: Some("pass the text to send, or - to read it from standard input".into()),
        });
    }
    let line = json!({ "type": "user", "message": { "role": "user", "content": text } });
    Ok(format!("{line}\n"))
}

/// What one line of the process's output says, as far as the server cares.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Said {
    /// The conversation's id, when the line names one.
    pub session_id: Option<String>,
    /// The answer to one message, when the line ends a turn: its text,
    /// empty when the turn ended with none (an error).
    pub answer: Option<String>,
}

/// Reads one line of output. A line that is not a JSON object says nothing.
#[must_use]
pub fn read(line: &str) -> Said {
    let Ok(Value::Object(object)) = serde_json::from_str::<Value>(line) else {
        return Said::default();
    };
    let session_id = object.get("session_id").and_then(Value::as_str);
    let session_id = session_id.filter(|id| !id.is_empty()).map(str::to_string);
    let answer = (object.get("type").and_then(Value::as_str) == Some("result"))
        .then(|| object.get("result").and_then(Value::as_str).unwrap_or_default().to_string());
    Said { session_id, answer }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fresh_start_names_its_id_and_a_resume_names_the_one_to_resume() {
        let fresh = args(Start::Fresh("abc"));
        let tail: Vec<&str> = fresh.iter().rev().take(2).map(String::as_str).collect();
        assert_eq!(tail, ["abc", "--session-id"]);
        let resume = args(Start::Resume("abc"));
        assert_eq!(resume[resume.len() - 2..], ["--resume", "abc"]);
        for flag in ["--print", "stream-json", "--verbose"] {
            assert!(fresh.iter().any(|a| a == flag), "{flag}");
        }
    }

    #[test]
    fn a_session_id_is_a_version_4_uuid() {
        assert_eq!(session_id([0; 16]), "00000000-0000-4000-8000-000000000000");
        assert_eq!(session_id([0xff; 16]), "ffffffff-ffff-4fff-bfff-ffffffffffff");
    }

    #[test]
    fn a_message_is_one_user_line_and_blank_text_is_invalid() {
        let line = message("say \"hi\"\nthen stop").unwrap();
        assert_eq!(line.matches('\n').count(), 1);
        let value: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(value["type"], "user");
        assert_eq!(value["message"], json!({ "role": "user", "content": "say \"hi\"\nthen stop" }));
        assert_eq!(message(" \n").unwrap_err().code(), "invalid");
    }

    #[test]
    fn output_names_the_session_and_a_result_ends_a_turn_with_its_answer() {
        let init = read(r#"{"type":"system","subtype":"init","session_id":"s1"}"#);
        assert_eq!(init, Said { session_id: Some("s1".into()), answer: None });
        let result =
            read(r#"{"type":"result","subtype":"success","session_id":"s1","result":"hi"}"#);
        assert_eq!(result, Said { session_id: Some("s1".into()), answer: Some("hi".into()) });
        let failed = read(r#"{"type":"result","subtype":"error_during_execution"}"#);
        assert_eq!(failed.answer.as_deref(), Some(""));
        for nothing in ["", "not json", "[1]", r#"{"type":"assistant","session_id":""}"#] {
            assert_eq!(read(nothing), Said::default(), "{nothing}");
        }
    }
}
