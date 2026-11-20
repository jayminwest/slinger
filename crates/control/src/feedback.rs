//! The feedback verbs: `feedback_add`, `feedback_list`, `feedback_resolve`
//! and `feedback_path`, their requests, replies and client methods
//! (docs/service.md, "Feedback"). The app adds a note with what it shows;
//! the CLI adds, lists and resolves them.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::{Client, RemoteError};

/// `feedback_list`: the newest notes, newest first: the open ones, or `all`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeedbackListAsk {
    #[serde(default, skip_serializing_if = "is_false")]
    pub all: bool,
    /// At most this many; every one when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u64>,
}

/// `feedback_resolve`: resolve the note saved at `ts`, with a note of how.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeedbackResolveAsk {
    pub ts: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// `feedback_path`: the file the notes go to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeedbackPathAsk {}

/// `feedback_add`: a note to save, and what the app showed when it was
/// left. Each context field is optional; the CLI sends none.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeedbackNote {
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub canvas: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page: Option<String>,
    /// The pen's tool.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool: Option<String>,
    /// How many items were selected.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected: Option<u64>,
}

/// A saved note, as the file holds it, and how it was resolved, if it was.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeedbackEntry {
    /// When it was saved, local time with its offset; it names the note.
    pub ts: String,
    pub text: String,
    pub build: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub canvas: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved: Option<FeedbackResolved>,
}

/// How a note was resolved: when, and the note given.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeedbackResolved {
    pub ts: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// The reply to `feedback_add`: the file and the note saved.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeedbackAdded {
    pub path: PathBuf,
    pub entry: FeedbackEntry,
}

/// The reply to `feedback_list`: the file and its notes,
/// newest first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeedbackEntries {
    pub path: PathBuf,
    pub entries: Vec<FeedbackEntry>,
}

/// The reply to `feedback_resolve`: the line appended.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeedbackResolution {
    pub path: PathBuf,
    /// The `ts` of the note resolved.
    pub resolves: String,
    pub ts: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// The reply to `feedback_path`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeedbackFile {
    pub path: PathBuf,
}

impl Client {
    /// Saves a note, with what the app shows.
    ///
    /// # Errors
    ///
    /// `invalid` for blank text or a file the daemon cannot write, and
    /// `unavailable` when the daemon does not answer.
    pub fn feedback_add(&self, note: FeedbackNote) -> Result<FeedbackAdded, RemoteError> {
        self.call(note)
    }

    /// The newest `limit` notes (every one when `None`), newest first: the
    /// open ones, or with `all` the resolved ones too. None is not an error.
    ///
    /// # Errors
    ///
    /// `invalid` for a file the daemon cannot read, and `unavailable` when
    /// the daemon does not answer.
    pub fn feedback_list(
        &self,
        all: bool,
        limit: Option<u64>,
    ) -> Result<FeedbackEntries, RemoteError> {
        self.call(FeedbackListAsk { all, limit })
    }

    /// Resolves the note saved at `ts`, with `note`.
    ///
    /// # Errors
    ///
    /// `not_found` when no note has that `ts`, `conflict` when it is
    /// resolved already, and `unavailable` when the daemon does not answer.
    pub fn feedback_resolve(
        &self,
        ts: &str,
        note: Option<String>,
    ) -> Result<FeedbackResolution, RemoteError> {
        self.call(FeedbackResolveAsk { ts: ts.into(), note })
    }

    /// The file the notes go to.
    ///
    /// # Errors
    ///
    /// `unavailable` when the daemon does not answer.
    pub fn feedback_path(&self) -> Result<PathBuf, RemoteError> {
        Ok(self.call(FeedbackPathAsk {})?.path)
    }
}

#[expect(
    clippy::trivially_copy_pass_by_ref,
    reason = "serde's skip_serializing_if passes a reference"
)]
fn is_false(b: &bool) -> bool {
    !b
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Request;
    use crate::wire::request_line;

    #[test]
    fn a_feedback_request_reads_back_as_itself_by_its_own_op() {
        let note = FeedbackNote {
            text: "slow".into(),
            canvas: Some("idea".into()),
            selected: Some(2),
            ..FeedbackNote::default()
        };
        for (ask, line) in [
            (
                Request::FeedbackAdd(note),
                r#"{"v":1,"op":"feedback_add","text":"slow","canvas":"idea","selected":2}"#,
            ),
            (
                Request::FeedbackList(FeedbackListAsk { all: false, limit: None }),
                r#"{"v":1,"op":"feedback_list"}"#,
            ),
            (
                Request::FeedbackList(FeedbackListAsk { all: true, limit: Some(5) }),
                r#"{"v":1,"op":"feedback_list","all":true,"limit":5}"#,
            ),
            (
                Request::FeedbackResolve(FeedbackResolveAsk {
                    ts: "t".into(),
                    note: Some("slinger-1".into()),
                }),
                r#"{"v":1,"op":"feedback_resolve","ts":"t","note":"slinger-1"}"#,
            ),
            (Request::FeedbackPath(FeedbackPathAsk {}), r#"{"v":1,"op":"feedback_path"}"#),
        ] {
            assert_eq!(request_line(&ask).unwrap(), format!("{line}\n"));
            let read = crate::read_request(&mut line.as_bytes()).unwrap();
            assert_eq!(read, Some(ask), "{line}");
        }
        let err = crate::read_request(&mut r#"{"v":1,"op":"feedback_add"}"#.as_bytes());
        assert_eq!(err.unwrap_err().code(), "bad_request");
    }
}
