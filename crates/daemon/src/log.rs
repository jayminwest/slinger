//! The daemon's log file (`--log`, docs/service.md "The daemon"): where its
//! stderr and every agent's go when nobody started it from a terminal. The
//! file is rotated once at start, never while it serves, so a reader never
//! sees it move. Pointing fd 2 at it is the binary's job (`main.rs`): the
//! library only opens the file, so a test that runs the daemon in-process
//! keeps its own stderr.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use slinger_core::feedback;

/// A log over this many bytes at start is moved aside to `<file>.1`.
pub const LIMIT: u64 = 1 << 20;

/// The build this slingerd was made from: a commit (`-dirty` if changed),
/// else `dev`.
#[must_use]
pub fn build() -> String {
    feedback::build(option_env!("SLINGER_BUILD_SHA"), option_env!("SLINGER_BUILD_DIRTY"))
}

/// Whether a log of `len` bytes is moved aside before the daemon writes to it.
#[must_use]
pub fn rotates(len: u64) -> bool {
    len > LIMIT
}

/// Where a rotated `path` goes: the same name with `.1` added.
#[must_use]
pub fn rotated(path: &Path) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(".1");
    PathBuf::from(name)
}

/// One line of the daemon's own: `<at> slingerd <version> (<build>) pid
/// <pid>: <what>`, with its newline.
#[must_use]
pub fn line(at: &str, build: &str, pid: u32, what: &str) -> String {
    format!("{at} slingerd {} ({build}) pid {pid}: {what}\n", env!("CARGO_PKG_VERSION"))
}

/// The log at `path`, open for appending: its folder made, and the file
/// moved aside to [`rotated`] first if it [`rotates`].
///
/// # Errors
///
/// When the folder cannot be made, the old file cannot be moved, or the
/// file cannot be opened.
pub fn open(path: &Path) -> io::Result<File> {
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        fs::create_dir_all(dir)?;
    }
    match fs::metadata(path) {
        Ok(meta) if rotates(meta.len()) => fs::rename(path, rotated(path))?,
        Ok(_) => {}
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => return Err(e),
    }
    OpenOptions::new().create(true).append(true).open(path)
}

/// Writes [`line`] for `what` to `log`, stamped now in local time.
///
/// # Errors
///
/// When the write fails.
pub fn say(mut log: &File, what: &str) -> io::Result<()> {
    let (unix_ms, offset) = crate::feedback::clock();
    let at = feedback::timestamp(unix_ms, offset);
    log.write_all(line(&at, &build(), std::process::id(), what).as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_file_over_the_limit_rotates() {
        assert!(!rotates(0));
        assert!(!rotates(LIMIT));
        assert!(rotates(LIMIT + 1));
        assert_eq!(rotated(Path::new("/d/slingerd.log")), Path::new("/d/slingerd.log.1"));
    }

    #[test]
    fn a_line_names_the_time_version_build_and_pid() {
        let said = line("2026-11-20T09:00:00.000-08:00", "abc1234", 42, "stopped");
        let want = format!(
            "2026-11-20T09:00:00.000-08:00 slingerd {} (abc1234) pid 42: stopped\n",
            env!("CARGO_PKG_VERSION")
        );
        assert_eq!(said, want);
    }

    #[test]
    fn open_makes_the_folder_appends_and_rotates_a_big_file_once() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sub/slingerd.log");
        say(&open(&path).unwrap(), "one").unwrap();
        say(&open(&path).unwrap(), "two").unwrap();
        let text = fs::read_to_string(&path).unwrap();
        assert_eq!(text.lines().count(), 2, "{text}");
        assert!(text.contains(": one\n") && text.ends_with(": two\n"), "{text}");

        let big = vec![b'x'; usize::try_from(LIMIT).unwrap() + 1];
        fs::write(&path, &big).unwrap();
        fs::write(rotated(&path), "older").unwrap();
        say(&open(&path).unwrap(), "fresh").unwrap();
        assert_eq!(fs::read(rotated(&path)).unwrap(), big, "the old .1 is replaced");
        assert!(fs::read_to_string(&path).unwrap().ends_with(": fresh\n"));
        assert!(open(dir.path()).is_err(), "a folder is no log");
    }
}
