//! The feedback file on disk (docs/service.md, "Feedback"): the IO, the
//! clock and the build label that `slinger_core::feedback` leaves out. Each
//! note and each resolution is one line, appended with one write to a file
//! opened for appending, so a crash or a second writer never harms the
//! lines before it.

use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Mutex, PoisonError};
use std::time::{SystemTime, UNIX_EPOCH};

use slinger_core::feedback::{self, Context, Entry, FILE, Resolution};

/// The checkout this slingerd was built from: `SLINGER_BUILD_SOURCE` at
/// build time, else the workspace root above this crate.
const SOURCE: Option<&str> = option_env!("SLINGER_BUILD_SOURCE");
const CRATE: &str = env!("CARGO_MANIFEST_DIR");

/// Why a feedback verb failed.
#[derive(Debug, thiserror::Error)]
pub enum FeedbackError {
    #[error(transparent)]
    Core(#[from] slinger_core::Error),
    #[error("cannot use feedback file {}: {source}", path.display())]
    Io { path: PathBuf, source: io::Error },
}

impl FeedbackError {
    /// Stable machine-readable code.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::Core(e) => e.code(),
            Self::Io { .. } => "invalid",
        }
    }

    /// What the caller can do about it, if anything.
    #[must_use]
    pub fn hint(&self) -> Option<String> {
        match self {
            Self::Core(e) => e.hint().map(str::to_string),
            Self::Io { .. } => Some("start slingerd with --feedback <FILE> it can write".into()),
        }
    }
}

type Result<T> = std::result::Result<T, FeedbackError>;

/// The feedback file and the build that writes to it. Safe to share
/// between threads: one verb reads and appends at a time.
#[derive(Debug)]
pub struct Feedback {
    path: PathBuf,
    build: String,
    lock: Mutex<()>,
}

impl Feedback {
    /// Notes go to `path`, made with its folder on the first note.
    #[must_use]
    pub fn new(path: PathBuf) -> Self {
        let build =
            feedback::build(option_env!("SLINGER_BUILD_SHA"), option_env!("SLINGER_BUILD_DIRTY"));
        Self { path, build, lock: Mutex::new(()) }
    }

    /// The file the notes go to.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Appends a note of `text` with `context`, saved now.
    ///
    /// # Errors
    ///
    /// `invalid` for blank text, and [`FeedbackError::Io`] when the file
    /// cannot be written.
    pub fn add(&self, text: &str, context: Context) -> Result<Entry> {
        let entry = feedback::note(text, context, now(), &self.build)?;
        let _held = self.lock.lock().unwrap_or_else(PoisonError::into_inner);
        self.append(&entry.line())?;
        Ok(entry)
    }

    /// The newest `limit` notes, newest first: the open ones, or `all`.
    /// No file yet is no notes.
    ///
    /// # Errors
    ///
    /// [`FeedbackError::Io`] when the file cannot be read.
    pub fn list(&self, all: bool, limit: usize) -> Result<Vec<Entry>> {
        Ok(feedback::read(&self.text()?, limit, all))
    }

    /// Appends a resolution of the note saved at `ts`, now, with `note`.
    ///
    /// # Errors
    ///
    /// `not_found` when no note has that `ts`, `conflict` when it is
    /// resolved already, and [`FeedbackError::Io`] when the file cannot be
    /// read or written.
    pub fn resolve(&self, ts: &str, note: Option<String>) -> Result<Resolution> {
        let _held = self.lock.lock().unwrap_or_else(PoisonError::into_inner);
        let resolution = feedback::resolve(&self.text()?, ts, now(), note)?;
        self.append(&resolution.line())?;
        Ok(resolution)
    }

    fn text(&self) -> Result<String> {
        match fs::read_to_string(&self.path) {
            Ok(text) => Ok(text),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(String::new()),
            Err(source) => Err(FeedbackError::Io { path: self.path.clone(), source }),
        }
    }

    fn append(&self, line: &str) -> Result<()> {
        let failed = |source| FeedbackError::Io { path: self.path.clone(), source };
        if let Some(dir) = self.path.parent().filter(|d| !d.as_os_str().is_empty()) {
            fs::create_dir_all(dir).map_err(failed)?;
        }
        let mut file =
            OpenOptions::new().create(true).append(true).open(&self.path).map_err(failed)?;
        file.write_all(line.as_bytes()).map_err(failed)
    }
}

/// Where notes go when `--feedback` names no file: `feedback.jsonl` in the
/// checkout this slingerd was built from, else in the data directory `data`.
#[must_use]
pub fn default_file(data: &Path) -> PathBuf {
    let built = SOURCE.filter(|s| !s.is_empty()).map(PathBuf::from);
    let checkout = built.or_else(|| Path::new(CRATE).ancestors().nth(2).map(Path::to_path_buf));
    checkout.filter(|dir| dir.is_dir()).unwrap_or_else(|| data.to_path_buf()).join(FILE)
}

/// Now, as local time with its UTC offset. The offset is what `date`
/// says, so it follows the system's time zone and its daylight saving;
/// UTC when `date` cannot say.
fn now() -> String {
    let unix = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let said = Command::new("date").arg("+%z").output().ok();
    let offset = said.and_then(|out| feedback::offset(&String::from_utf8_lossy(&out.stdout)));
    feedback::timestamp(i64::try_from(unix).unwrap_or(i64::MAX), offset.unwrap_or(0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_is_the_checkout_this_was_built_from() {
        let data = Path::new("/nonexistent/data");
        let file = default_file(data);
        assert_eq!(file.file_name().and_then(|n| n.to_str()), Some(FILE));
        assert!(file.parent().is_some_and(|dir| dir.join("Cargo.toml").is_file()), "{file:?}");
    }

    #[test]
    fn notes_append_resolve_and_read_back_and_a_bad_file_says_so() {
        let dir = tempfile::tempdir().unwrap();
        let notes = Feedback::new(dir.path().join("sub/feedback.jsonl"));
        assert_eq!(notes.list(false, 5).unwrap(), []);
        let saved = notes.add("slow", Context::default()).unwrap();
        assert!(saved.ts.len() == 25 && saved.ts.contains('T'), "{}", saved.ts);
        notes.resolve(&saved.ts, None).unwrap();
        assert_eq!(notes.list(false, 5).unwrap(), []);
        assert_eq!(notes.list(true, 5).unwrap().len(), 1);
        let lines = fs::read_to_string(notes.path()).unwrap();
        assert_eq!(lines.lines().count(), 2);

        let unusable = Feedback::new(dir.path().to_path_buf());
        let err = unusable.add("x", Context::default()).unwrap_err();
        assert_eq!((err.code(), err.hint().is_some()), ("invalid", true));
        assert_eq!(unusable.list(true, 1).unwrap_err().code(), "invalid");
    }
}
