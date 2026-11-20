//! `slingerd`: a thin `main` over the daemon library.

use std::fs::File;
use std::io;
use std::process::ExitCode;

use slingerd as daemon;

fn main() -> ExitCode {
    // Unlocked handles: each write takes the lock for that write alone. A
    // lock held across `run` would block every other thread's `eprintln!`
    // for the daemon's whole life.
    let (mut out, mut err) = (io::stdout(), io::stderr());
    ExitCode::from(daemon::run_with(std::env::args_os(), &mut out, &mut err, to_stderr))
}

/// Points this process's fd 2 at the `--log` file, so the daemon's warnings
/// and every agent's stderr (agents inherit it) land there. Only the binary
/// does this: a test that runs the daemon in-process keeps its stderr.
fn to_stderr(log: &File) -> io::Result<()> {
    rustix::stdio::dup2_stderr(log).map_err(io::Error::from)
}
