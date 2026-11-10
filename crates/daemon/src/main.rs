//! `slingerd`: a thin `main` over the daemon library.

use std::io;
use std::process::ExitCode;

use slingerd as daemon;

fn main() -> ExitCode {
    let (mut out, mut err) = (io::stdout().lock(), io::stderr().lock());
    ExitCode::from(daemon::run(std::env::args_os(), &mut out, &mut err))
}
