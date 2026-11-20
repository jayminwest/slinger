//! The UI catalog verbs: `ui_put`, `ui_list`, `ui_show` and `ui_export`
//! (docs/service.md). Showing a UI is in [`crate::ui`].

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::client::absolute;
use crate::{Client, RemoteError};

/// `ui_put`: store the HTML file at `path` as the next version of the UI
/// `name`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UiPutAsk {
    pub name: String,
    pub path: PathBuf,
    pub source: String,
}

/// `ui_list`: the latest version of every UI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UiListAsk {}

/// `ui_show`: every version of the UI `name`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UiShowAsk {
    pub name: String,
}

/// `ui_export`: write the version `ui` names (`name`, `name@3` or a blob
/// id) into the directory `dir`, as `<name>.v<version>.html`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UiExportAsk {
    pub ui: String,
    pub dir: PathBuf,
}

/// One version of an agent UI, as the record holds it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ui {
    pub name: String,
    pub version: u64,
    /// The code's blob id.
    pub hash: String,
    /// The code's size.
    pub bytes: u64,
    /// Who put it.
    pub source: String,
    /// When, in milliseconds since the Unix epoch, UTC.
    pub time: u64,
}

/// The reply to `ui_put`: the version that holds the bytes, and whether
/// this put made it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UiPut {
    pub ui: Ui,
    pub new: bool,
}

/// The reply to `ui_list` and `ui_show`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Uis {
    pub uis: Vec<Ui>,
}

/// The reply to `ui_export`: the version written and its file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Exported {
    pub ui: Ui,
    pub path: PathBuf,
}

impl Client {
    /// Stores the HTML file at `file` as the next version of the UI `name`,
    /// put by `source`. Bytes equal to the latest version's add none.
    ///
    /// # Errors
    ///
    /// `invalid` for a name or code the rules refuse (docs/design/live-items.md),
    /// `not_found` when there is no such file, and `unavailable` when the
    /// daemon does not answer.
    pub fn ui_put(&self, name: &str, file: &Path, source: &str) -> Result<UiPut, RemoteError> {
        let path = absolute(file)?;
        self.call(UiPutAsk { name: name.into(), path, source: source.into() })
    }

    /// The latest version of every UI, by name. With `name`, every version of
    /// that UI, oldest first.
    ///
    /// # Errors
    ///
    /// `not_found` for a name with no versions, and `unavailable` when the
    /// daemon does not answer.
    pub fn ui_list(&self, name: Option<&str>) -> Result<Vec<Ui>, RemoteError> {
        let reply = match name {
            None => self.call(UiListAsk {})?,
            Some(name) => self.call(UiShowAsk { name: name.into() })?,
        };
        Ok(reply.uis)
    }

    /// Writes the version `ui` names into the directory `dir`, creating it.
    ///
    /// # Errors
    ///
    /// `not_found` when nothing matches `ui`, `invalid` for a reference of
    /// no known form or a directory the daemon cannot write, and
    /// `unavailable` when the daemon does not answer.
    pub fn ui_export(&self, ui: &str, dir: &Path) -> Result<Exported, RemoteError> {
        self.call(UiExportAsk { ui: ui.into(), dir: absolute(dir)? })
    }
}
