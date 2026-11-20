//! What the store keeps of the daemon's process for `daemon_info` and
//! `daemon_stop`: the data directory it opened and the `--log` file.

use std::path::{Path, PathBuf};

use super::Store;

impl Store {
    /// The data directory: the record, the canvases and the socket's name.
    #[must_use]
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// The file the daemon logs to (`--log`), as `daemon_info` gives it.
    #[must_use]
    pub fn with_log(mut self, path: Option<PathBuf>) -> Self {
        self.log = path;
        self
    }

    /// The daemon's `--log` file, if it has one.
    #[must_use]
    pub fn log(&self) -> Option<&Path> {
        self.log.as_deref()
    }
}
