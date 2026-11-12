//! The feedback verbs: `feedback_add`, `feedback_list`, `feedback_resolve`
//! and `feedback_path`, their requests, replies and client methods
//! (docs/service.md, "Feedback"). The app adds a note with what it shows;
//! the CLI adds, lists and resolves them.
//!
//! [`crate::Request`] reads them as one variant, [`FeedbackAsk`], and does
//! not write them: a client writes [`FeedbackAsk`] itself.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::{Client, RemoteError};

/// A feedback request, as a client writes it and the daemon reads it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum FeedbackAsk {
    /// Save a note, with what the app shows.
    FeedbackAdd(FeedbackNote),
    /// The newest notes, newest first: the open ones, or `all`.
    FeedbackList {
        #[serde(default, skip_serializing_if = "is_false")]
        all: bool,
        /// At most this many; every one when absent.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        limit: Option<u64>,
    },
    /// Resolve the note saved at `ts`, with a note of how.
    FeedbackResolve {
        ts: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        note: Option<String>,
    },
    /// The file the notes go to.
    FeedbackPath,
}

/// A note to save, and what the app showed when it was left. Each context
/// field is optional; the CLI sends none.
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

/// The reply to [`FeedbackAsk::FeedbackAdd`]: the file and the note saved.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeedbackAdded {
    pub path: PathBuf,
    pub entry: FeedbackEntry,
}

/// The reply to [`FeedbackAsk::FeedbackList`]: the file and its notes,
/// newest first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeedbackEntries {
    pub path: PathBuf,
    pub entries: Vec<FeedbackEntry>,
}

/// The reply to [`FeedbackAsk::FeedbackResolve`]: the line appended.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeedbackResolution {
    pub path: PathBuf,
    /// The `ts` of the note resolved.
    pub resolves: String,
    pub ts: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// The reply to [`FeedbackAsk::FeedbackPath`].
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
        self.call(&FeedbackAsk::FeedbackAdd(note))
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
        self.call(&FeedbackAsk::FeedbackList { all, limit })
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
        self.call(&FeedbackAsk::FeedbackResolve { ts: ts.into(), note })
    }

    /// The file the notes go to.
    ///
    /// # Errors
    ///
    /// `unavailable` when the daemon does not answer.
    pub fn feedback_path(&self) -> Result<PathBuf, RemoteError> {
        let reply: FeedbackFile = self.call(&FeedbackAsk::FeedbackPath)?;
        Ok(reply.path)
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
                FeedbackAsk::FeedbackAdd(note),
                r#"{"v":1,"op":"feedback_add","text":"slow","canvas":"idea","selected":2}"#,
            ),
            (
                FeedbackAsk::FeedbackList { all: false, limit: None },
                r#"{"v":1,"op":"feedback_list"}"#,
            ),
            (
                FeedbackAsk::FeedbackList { all: true, limit: Some(5) },
                r#"{"v":1,"op":"feedback_list","all":true,"limit":5}"#,
            ),
            (
                FeedbackAsk::FeedbackResolve { ts: "t".into(), note: Some("slinger-1".into()) },
                r#"{"v":1,"op":"feedback_resolve","ts":"t","note":"slinger-1"}"#,
            ),
            (FeedbackAsk::FeedbackPath, r#"{"v":1,"op":"feedback_path"}"#),
        ] {
            assert_eq!(request_line(&ask).unwrap(), format!("{line}\n"));
            let read = crate::read_request(&mut line.as_bytes()).unwrap();
            assert_eq!(read, Some(Request::Feedback(ask)), "{line}");
        }
        let err = crate::read_request(&mut r#"{"v":1,"op":"feedback_add"}"#.as_bytes());
        assert_eq!(err.unwrap_err().code(), "bad_request");
    }
}
