//! `slingerd`: a thin `main` over the daemon library.

use std::io;
use std::process::ExitCode;

use slingerd as daemon;

fn main() -> ExitCode {
    // Unlocked handles: each write takes the lock for that write alone. A
    // lock held across `run` would block every other thread's `eprintln!`
    // for the daemon's whole life.
    let (mut out, mut err) = (io::stdout(), io::stderr());
    ExitCode::from(daemon::run(std::env::args_os(), &mut out, &mut err))
}
