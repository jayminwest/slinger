//! Writing the workspaces directory's files: whole or not at all.

use std::fs::{self, DirBuilder, File};
use std::io::Write;
use std::os::unix::fs::DirBuilderExt;
use std::path::Path;

use super::{Result, failed};

/// Makes `dir` and its parents (mode 0700) when they are not there.
pub(crate) fn mkdir(dir: &Path) -> Result<()> {
    DirBuilder::new().recursive(true).mode(0o700).create(dir).map_err(failed("make", dir))
}

/// The file whole or not at all: written aside, synced, then renamed in.
pub(crate) fn write_synced(path: &Path, bytes: &[u8]) -> Result<()> {
    let name = path.file_name().map(|n| n.to_string_lossy()).unwrap_or_default();
    let temp = path.with_file_name(format!(".{name}.new"));
    let written =
        File::create(&temp).and_then(|mut f| f.write_all(bytes).and_then(|()| f.sync_all()));
    written.map_err(failed("write", &temp))?;
    fs::rename(&temp, path).map_err(failed("write", path))
}
