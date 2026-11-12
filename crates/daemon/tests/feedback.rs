//! The real binary picks the feedback file: `--feedback` names it, a
//! relative one from where slingerd started, and without it notes go to the
//! checkout slingerd was built from.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use slinger_control::{Client, FeedbackNote};

/// A slingerd started in `cwd` with a fresh data directory and `args`,
/// stopped with SIGTERM when dropped.
struct Daemon {
    child: Option<Child>,
    socket: PathBuf,
    data: tempfile::TempDir,
}

impl Daemon {
    fn start(tag: &str, cwd: &Path, args: &[&str]) -> std::io::Result<Self> {
        let socket = std::env::temp_dir().join(format!("f{tag}-{}.sock", std::process::id()));
        let data = tempfile::tempdir()?;
        let child = Command::new(env!("CARGO_BIN_EXE_slingerd"))
            .current_dir(cwd)
            .arg("--socket")
            .arg(&socket)
            .arg("--data-dir")
            .arg(data.path())
            .args(args)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
        let daemon = Self { child: Some(child), socket, data };
        let deadline = Instant::now() + Duration::from_secs(10);
        while let Err(e) = daemon.client().canvas_list() {
            assert!(Instant::now() < deadline, "the daemon never answered: {e}");
            std::thread::sleep(Duration::from_millis(20));
        }
        Ok(daemon)
    }

    fn client(&self) -> Client {
        Client::new(&self.socket)
    }
}

impl Drop for Daemon {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let pid = child.id().to_string();
            let _ = Command::new("kill").arg("-TERM").arg(pid).status();
            let _ = child.wait();
        }
        let _ = std::fs::remove_file(format!("{}.lock", self.socket.display()));
    }
}

#[test]
fn a_relative_feedback_file_is_from_where_slingerd_started() {
    let work = tempfile::tempdir().unwrap();
    let daemon = Daemon::start("rel", work.path(), &["--feedback", "notes/fb.jsonl"]).unwrap();
    let client = daemon.client();

    let path = client.feedback_path().unwrap();
    let expected = std::fs::canonicalize(work.path()).unwrap().join("notes/fb.jsonl");
    assert_eq!(path, expected);
    let added = client.feedback_add(FeedbackNote { text: "hi".into(), ..FeedbackNote::default() });
    assert_eq!(added.unwrap().path, expected);
    let saved = std::fs::read_to_string(work.path().join("notes/fb.jsonl")).unwrap();
    assert_eq!(saved.lines().count(), 1, "{saved}");
    assert!(!daemon.data.path().join("feedback.jsonl").exists());
}

#[test]
fn without_feedback_notes_go_to_the_checkout_slingerd_was_built_from() {
    let work = tempfile::tempdir().unwrap();
    let daemon = Daemon::start("def", work.path(), &[]).unwrap();

    // Only asks where: a note here would land in the real checkout.
    let path = daemon.client().feedback_path().unwrap();
    let checkout = option_env!("SLINGER_BUILD_SOURCE")
        .filter(|s| !s.is_empty())
        .map_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."), PathBuf::from);
    assert_eq!(path.file_name().and_then(|n| n.to_str()), Some("feedback.jsonl"));
    let dir = std::fs::canonicalize(path.parent().unwrap()).unwrap();
    assert_eq!(dir, std::fs::canonicalize(checkout).unwrap());
}
