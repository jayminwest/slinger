//! The blob verbs: `blob_put` and `blob_get` (docs/service.md).
//! Bytes travel as a file on this machine, never in the line.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::client::absolute;
use crate::{Client, RemoteError};

/// `blob_put`: store the bytes of the file at `path` as a blob.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlobPutAsk {
    pub path: PathBuf,
}

/// `blob_get`: write the bytes of the blob `id` to the file at `path`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlobGetAsk {
    pub id: String,
    pub path: PathBuf,
}

/// The reply to `blob_put`: the blob's id, `sha256:<hex>`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stored {
    pub id: String,
}

/// The reply to `blob_get`: how many bytes were written.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Written {
    pub size: u64,
}

impl Client {
    /// Stores the bytes of the file at `file` and returns the blob's id.
    /// Storing bytes already held returns the same id. A relative path is
    /// taken from this process's working directory.
    ///
    /// # Errors
    ///
    /// `not_found` when there is no such file, `invalid` when the daemon
    /// cannot read it, and `unavailable` when the daemon does not answer.
    pub fn blob_put(&self, file: &Path) -> Result<String, RemoteError> {
        Ok(self.call(BlobPutAsk { path: absolute(file)? })?.id)
    }

    /// Writes the bytes of the blob `id` to the file at `file`, replacing it,
    /// and returns how many there were.
    ///
    /// # Errors
    ///
    /// `not_found` when the record holds no such blob, `invalid` for an id
    /// that is not one or a file the daemon cannot write, and `unavailable`
    /// when the daemon does not answer.
    pub fn blob_get(&self, id: &str, file: &Path) -> Result<u64, RemoteError> {
        Ok(self.call(BlobGetAsk { id: id.to_string(), path: absolute(file)? })?.size)
    }
}
