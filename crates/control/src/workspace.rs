//! The workspace verbs: `workspace_open`, `workspace_list`, `workspace_show`
//! and `workspace_close` (docs/service.md, "Workspaces").

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::client::absolute;
use crate::{Client, RemoteError};

/// `workspace_open`: open a workspace: the directory `dir`, or one with no
/// root named `name`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceOpenAsk {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dir: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// `workspace_list`: the open workspaces.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceListAsk {}

/// `workspace_show`: one open workspace, by name or by the directory of its
/// file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceShowAsk {
    pub workspace: String,
}

/// `workspace_close`: close a workspace, by name or by the directory of its
/// file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceCloseAsk {
    pub workspace: String,
}

/// One open workspace, as the daemon read it from its definition file. A
/// workspace whose file the daemon cannot use has an `error`, its `dir`,
/// and its `name` and `canvas` only if the file says them; nothing else.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Workspace {
    /// Its name. Empty only for a broken file that names none.
    #[serde(default)]
    pub name: String,
    /// Its tag on events in the record.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub tag: String,
    /// Its default canvas's id.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub canvas: String,
    /// The directory that holds its definition file: the primary root, or a
    /// directory in the daemon's data directory for a workspace with no root.
    pub dir: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<WorkspaceAgent>,
    #[serde(default)]
    pub roots: Vec<WorkspaceRoot>,
    /// Why the daemon cannot use its file. Every verb on it fails with this.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<WorkspaceFileError>,
}

/// Why an open workspace's definition file cannot be used: the error every
/// verb on the workspace fails with.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceFileError {
    /// The stable code: `invalid` for a file that breaks a rule.
    pub code: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
    /// The definition file.
    pub file: PathBuf,
    /// The field that broke a rule (`agent.tools`), when one did.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
}

/// The agent of a workspace.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceAgent {
    pub name: String,
    #[serde(default)]
    pub durable: bool,
    /// The names of its profiles, in its file's order (`profile_list` has
    /// the rest of each).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub profiles: Vec<String>,
}

/// One directory of a workspace.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceRoot {
    pub machine: String,
    pub path: String,
    #[serde(default)]
    pub primary: bool,
}

/// The reply to `workspace_open`: the workspace, and whether this
/// open made its definition file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceOpened {
    pub workspace: Workspace,
    pub new: bool,
}

/// The reply to `workspace_list`: the open workspaces, by name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Workspaces {
    pub workspaces: Vec<Workspace>,
}

/// The reply to `workspace_show`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceInfo {
    pub workspace: Workspace,
}

/// The reply to `workspace_close`: the directory of the file of
/// the workspace closed. The file stays.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceClosed {
    pub dir: PathBuf,
}

impl Client {
    /// Opens the directory `dir` as a workspace, making its definition file
    /// and its canvas when it has none; or, with no `dir`, the workspace with
    /// no root named `name`. Opening an open workspace returns it.
    ///
    /// # Errors
    ///
    /// `invalid` with neither, `not_found` for a directory that is not
    /// there, `conflict` when the workspace breaks a rule over the open ones
    /// (docs/workspace.md), and `unavailable` when the daemon does not answer.
    pub fn workspace_open(
        &self,
        dir: Option<&Path>,
        name: Option<&str>,
    ) -> Result<WorkspaceOpened, RemoteError> {
        let dir = dir.map(absolute).transpose()?;
        self.call(WorkspaceOpenAsk { dir, name: name.map(str::to_string) })
    }

    /// The open workspaces, by name; one whose file the daemon cannot use
    /// is listed with its `error`.
    ///
    /// # Errors
    ///
    /// `unavailable` when the daemon does not answer.
    pub fn workspace_list(&self) -> Result<Vec<Workspace>, RemoteError> {
        Ok(self.call(WorkspaceListAsk {})?.workspaces)
    }

    /// One open workspace, by name or by the absolute directory of its file.
    ///
    /// # Errors
    ///
    /// `not_found` for no such open workspace, the error of its file when
    /// the daemon cannot use it, and `unavailable` when the daemon does not
    /// answer.
    pub fn workspace_show(&self, workspace: &str) -> Result<Workspace, RemoteError> {
        Ok(self.call(WorkspaceShowAsk { workspace: workspace.into() })?.workspace)
    }

    /// Closes a workspace, by name or by the absolute directory of its file,
    /// and returns that directory. Its file, its canvas and its agent stay.
    ///
    /// # Errors
    ///
    /// As [`Client::workspace_show`].
    pub fn workspace_close(&self, workspace: &str) -> Result<PathBuf, RemoteError> {
        Ok(self.call(WorkspaceCloseAsk { workspace: workspace.into() })?.dir)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Request;
    use crate::wire::request_line;

    #[test]
    fn a_workspace_request_reads_back_as_itself() {
        let open = |dir: Option<&str>, name: Option<&str>| {
            let (dir, name) = (dir.map(Into::into), name.map(str::to_string));
            Request::WorkspaceOpen(WorkspaceOpenAsk { dir, name })
        };
        for request in [
            open(Some("/a"), Some("n")),
            open(None, None),
            Request::WorkspaceList(WorkspaceListAsk {}),
            Request::WorkspaceShow(WorkspaceShowAsk { workspace: "w".into() }),
            Request::WorkspaceClose(WorkspaceCloseAsk { workspace: "/d".into() }),
        ] {
            let line = request_line(&request).unwrap();
            let read = crate::read_request(&mut line.as_bytes()).unwrap();
            assert_eq!(read, Some(request), "{line}");
        }
        let line = request_line(&open(None, Some("n")));
        assert_eq!(line.unwrap(), "{\"v\":1,\"op\":\"workspace_open\",\"name\":\"n\"}\n");
    }
}
