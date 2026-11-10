//! The `slinger` daemon. It owns the IO that the core leaves out (the socket,
//! signals, startup) and serves the core over a local control socket
//! (docs/service.md). It holds no domain logic: each verb is one call into
//! the core.
//!
//! [`run`] is the whole program: argv in, exit code out. [`Server`] is the
//! socket alone, for tests and for other hosts of the same protocol.

mod server;
#[cfg(test)]
mod tests;

use std::ffi::OsString;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use clap::Parser;
use clap::error::ErrorKind;
use signal_hook::consts::{SIGINT, SIGTERM};
use signal_hook::iterator::Signals;

use slinger_control as control;

pub use server::{Server, StartError, Stopper};

const HELP_HINT: &str = "run 'slingerd --help'";
/// Help for `--socket`: a string, because a doc comment may not hold `$XDG_RUNTIME_DIR` bare.
const SOCKET_HELP: &str =
    "Control socket path [default: in $XDG_RUNTIME_DIR, else in the temporary directory]";

/// Exit codes, the same table as the CLI's (docs/service.md).
mod exit {
    pub const OK: u8 = 0;
    pub const ERROR: u8 = 1;
    pub const USAGE: u8 = 2;
    pub const CONFLICT: u8 = 4;
}

#[derive(Parser)]
#[command(
    name = "slingerd",
    about = "The slinger daemon: serves the core over a local control socket.",
    version,
    disable_version_flag = true
)]
struct Args {
    #[arg(long, value_name = "PATH", help = SOCKET_HELP)]
    socket: Option<PathBuf>,
    /// Print the version
    #[arg(short = 'v', long)]
    version: bool,
}

/// Runs the daemon until SIGTERM or SIGINT. Returns its exit code. It says
/// nothing on success; a failure is `✗ message` and a hint on `err`.
pub fn run<I, T>(args: I, out: &mut dyn Write, err: &mut dyn Write) -> u8
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    // If the report itself cannot be written, the exit code is all that is left.
    report(Args::try_parse_from(args), out, err).unwrap_or(exit::ERROR)
}

/// A failure as `run` reports it: the exit code, the message and the hint.
type Failure = (u8, String, Option<String>);

fn report(
    args: Result<Args, clap::Error>,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> io::Result<u8> {
    let (code, message, hint) = match args {
        Ok(args) if args.version => {
            writeln!(out, "{}", env!("CARGO_PKG_VERSION"))?;
            return Ok(exit::OK);
        }
        Ok(args) => match serve(&args.socket.unwrap_or_else(control::default_socket)) {
            Ok(()) => return Ok(exit::OK),
            Err(failure) => failure,
        },
        Err(e) if matches!(e.kind(), ErrorKind::DisplayHelp | ErrorKind::DisplayVersion) => {
            write!(out, "{}", e.render())?;
            return Ok(exit::OK);
        }
        Err(e) => {
            let rendered = e.render().to_string();
            let message = rendered.lines().next().unwrap_or_default();
            let message = message.trim_start_matches("error: ").to_string();
            (exit::USAGE, message, Some(HELP_HINT.to_string()))
        }
    };
    writeln!(err, "✗ {message}")?;
    if let Some(hint) = hint {
        writeln!(err, "  {hint}")?;
    }
    Ok(code)
}

fn serve(socket: &Path) -> Result<(), Failure> {
    let internal = |e: io::Error| (exit::ERROR, e.to_string(), None);
    let server = Server::bind(socket).map_err(|e| {
        let code = if e.code() == "conflict" { exit::CONFLICT } else { exit::ERROR };
        (code, e.to_string(), e.hint())
    })?;
    let stopper = server.stopper();
    let mut signals = Signals::new([SIGTERM, SIGINT]).map_err(internal)?;
    std::thread::spawn(move || {
        if signals.forever().next().is_some() {
            stopper.stop();
        }
    });
    server.serve().map_err(internal)
}
