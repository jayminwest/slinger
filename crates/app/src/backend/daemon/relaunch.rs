//! Opens the app again after it has quit (`restart-slinger`): a shell in
//! a process group of its own waits for the app's pid to go, then runs
//! `open` on the bundle, so two windows never show at once and the new
//! app finds no daemon and starts the one it holds.

use std::io;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Command, Stdio};

/// Waits for `$1` to exit, then runs `$2` on `$3`.
const WAIT: &str = r#"while kill -0 "$1" 2>/dev/null; do sleep 0.1; done; exec "$2" "$3""#;

/// Runs `opener` on `bundle` once the process `pid` is gone. Returns at
/// once; the helper outlives the caller.
///
/// # Errors
///
/// The shell could not be run.
pub fn after(pid: u32, opener: &Path, bundle: &Path) -> io::Result<()> {
    Command::new("/bin/sh")
        .args(["-c", WAIT, "sh", &pid.to_string()])
        .args([opener, bundle])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
        .map(drop)
}

#[cfg(test)]
mod tests {
    use std::thread;
    use std::time::{Duration, Instant};

    use super::*;

    #[test]
    fn the_opener_runs_on_the_bundle_only_once_the_app_is_gone() {
        let dir = tempfile::tempdir().unwrap();
        let bundle = dir.path().join("opened");
        let mut app = Command::new("/bin/sleep").arg("30").spawn().unwrap();
        after(app.id(), Path::new("/usr/bin/touch"), &bundle).unwrap();
        thread::sleep(Duration::from_millis(400));
        assert!(!bundle.exists(), "the app still runs");
        app.kill().unwrap();
        app.wait().unwrap();
        let until = Instant::now() + Duration::from_secs(10);
        while !bundle.exists() && Instant::now() < until {
            thread::sleep(Duration::from_millis(20));
        }
        assert!(bundle.exists(), "opened after the app quit");
    }
}
