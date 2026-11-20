//! The `slinger` daemon. It owns the IO that the core leaves out (the socket,
//! signals, startup, the record's file) and serves the core over a local
//! control socket (docs/service.md). It holds no domain logic: each verb is
//! one call into the core.
//!
//! [`run`] is the whole program: argv in, exit code out. [`Server`] is the
//! socket alone, for tests and for other hosts of the same protocol.
//! [`Store`] is this machine's record on disk.

mod canvases;
mod feedback;
mod listeners;
mod lock;
pub mod log;
mod processes;
mod scratch;
mod server;
mod sessions;
mod store;
#[cfg(test)]
mod tests;
mod workspaces;

use std::ffi::OsString;
use std::fs::File;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use clap::Parser;
use clap::error::ErrorKind;
use signal_hook::consts::{SIGHUP, SIGINT, SIGTERM};
use signal_hook::iterator::Signals;

use slinger_control as control;

pub use canvases::{CanvasError, Canvases};
pub use feedback::{Feedback, FeedbackError};
pub use listeners::{Listeners, Listening};
pub use processes::{ProcessError, Processes, Shown};
pub use server::{Server, StartError, Stopper};
pub use sessions::{SessionError, Sessions};
pub use store::{CLAUDE, Query, SLINGER, Store, StoreError, default_data_dir, machine_name};
pub use workspaces::{Broken, Listed, Opened, WorkspaceError, Workspaces};

const HELP_HINT: &str = "run 'slingerd --help'";
/// Help for `--socket`: a string, because a doc comment may not hold `$XDG_RUNTIME_DIR` bare.
const SOCKET_HELP: &str =
    "Control socket path [default: in $XDG_RUNTIME_DIR, else in the temporary directory]";
/// Help for `--data-dir`, for the same reason.
const DATA_DIR_HELP: &str = "Directory that holds the record [default: $XDG_DATA_HOME/slinger, \
     else ~/Library/Application Support/slinger on macOS, ~/.local/share/slinger elsewhere]";
/// Help for `--feedback`, for the same reason.
const FEEDBACK_HELP: &str = "File that feedback notes go to [default: feedback.jsonl in the \
     checkout slingerd was built from, else in the data directory]";
/// Help for `--log`.
const LOG_HELP: &str = "File that slingerd's and its agents' stderr go to, moved aside to \
     <PATH>.1 at start when over 1 MiB [default: stderr]";

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
    #[arg(long, value_name = "PATH", help = DATA_DIR_HELP)]
    data_dir: Option<PathBuf>,
    /// Let the data directory be in a temporary directory: the record may be lost
    #[arg(long)]
    scratch: bool,
    /// The program each agent's process runs: Claude Code
    #[arg(long, value_name = "PATH", default_value = CLAUDE)]
    claude: PathBuf,
    /// The slinger CLI each agent's hooks run [default: the one beside slingerd, else slinger on PATH]
    #[arg(long, value_name = "PATH")]
    slinger: Option<PathBuf>,
    #[arg(long, value_name = "PATH", help = FEEDBACK_HELP)]
    feedback: Option<PathBuf>,
    #[arg(long, value_name = "PATH", help = LOG_HELP)]
    log: Option<PathBuf>,
    /// Print the version
    #[arg(short = 'v', long)]
    version: bool,
}

/// What becomes of the process's stderr once a `--log` file is open: the
/// binary points fd 2 at it; a host that runs the daemon in-process (a
/// test) keeps its own stderr.
pub type Redirect = fn(&File) -> io::Result<()>;

/// Runs the daemon until SIGTERM or SIGINT. Returns its exit code. It says
/// nothing on success; a failure is `✗ message` and a hint on `err`. A
/// `--log` file gets the daemon's own lines; the process's stderr stays.
pub fn run<I, T>(args: I, out: &mut dyn Write, err: &mut dyn Write) -> u8
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    run_with(args, out, err, |_| Ok(()))
}

/// [`run`], with `redirect` called on the `--log` file once it is open,
/// before the daemon writes anything there or starts an agent.
pub fn run_with<I, T>(args: I, out: &mut dyn Write, err: &mut dyn Write, redirect: Redirect) -> u8
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    // If the report itself cannot be written, the exit code is all that is left.
    report(Args::try_parse_from(args), out, err, redirect).unwrap_or(exit::ERROR)
}

/// A failure as `run` reports it: the exit code, the message and the hint.
type Failure = (u8, String, Option<String>);

fn report(
    args: Result<Args, clap::Error>,
    out: &mut dyn Write,
    err: &mut dyn Write,
    redirect: Redirect,
) -> io::Result<u8> {
    let (code, message, hint) = match args {
        Ok(args) if args.version => {
            writeln!(out, "{}", env!("CARGO_PKG_VERSION"))?;
            return Ok(exit::OK);
        }
        Ok(args) => {
            let socket = args.socket.unwrap_or_else(control::default_socket);
            let slinger = args.slinger.unwrap_or_else(beside_me);
            let programs = (args.claude, slinger);
            // A relative --log is from where slingerd started.
            let log = args.log.map(|f| (std::path::absolute(&f).unwrap_or(f), redirect));
            let served = data_dir(args.data_dir, args.scratch)
                .and_then(|dir| serve(&socket, &dir, programs, args.feedback, log, err));
            match served {
                Ok(()) => return Ok(exit::OK),
                Err(failure) => failure,
            }
        }
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

/// Writes one warning per open workspace whose file cannot be used: the
/// file, what is wrong and the hint. The others serve; so does the daemon.
fn warn_broken(workspaces: &Workspaces, err: &mut dyn Write) -> io::Result<()> {
    let Ok(all) = workspaces.list() else { return Ok(()) };
    for broken in all.into_iter().filter_map(Listed::broken) {
        writeln!(err, "slingerd: workspace not loaded: {}", broken.error)?;
        if let Some(hint) = broken.error.hint() {
            writeln!(err, "  {hint}")?;
        }
    }
    Ok(())
}

/// The `slinger` installed beside this `slingerd`, else the name alone, to
/// look up on `PATH`.
fn beside_me() -> PathBuf {
    let beside = std::env::current_exe().ok().and_then(|me| Some(me.parent()?.join(SLINGER)));
    beside.filter(|p| p.is_file()).unwrap_or_else(|| SLINGER.into())
}

/// The directory the record goes in: `given`, else the default. Absolute,
/// so the paths the daemon gives out (`canvas_show`) are. One in a temporary
/// directory is refused unless `scratch`: the system empties it, and every
/// canvas with it (slinger-491a02).
fn data_dir(given: Option<PathBuf>, scratch: bool) -> Result<PathBuf, Failure> {
    let dir = given
        .map_or_else(default_data_dir, Ok)
        .map_err(|e| (exit::ERROR, e.to_string(), e.hint()))?;
    let dir = std::path::absolute(&dir).unwrap_or(dir);
    match scratch::temporary_root(&dir).filter(|_| !scratch) {
        Some(root) => Err((
            exit::USAGE,
            format!(
                "data directory {} is in {}, which the system empties",
                dir.display(),
                root.display()
            ),
            Some("pass a --data-dir that lasts, or --scratch if the record may be lost".into()),
        )),
        None => Ok(dir),
    }
}

/// Serves the record in `data_dir` on `socket`, with the agents' `claude`
/// and `slinger` programs, writing feedback to `feedback` (by default
/// [`feedback::default_file`]). Names each open workspace whose file it
/// cannot use on `err`, once, before it serves. Names its socket in the data
/// directory while it serves, so clients find it on any path. With a `log`,
/// once the socket and the record are its own (a second daemon never moves
/// the first one's log), it opens the file, hands it to the redirect and
/// says it serves.
fn serve(
    socket: &Path,
    dir: &Path,
    (claude, slinger): (PathBuf, PathBuf),
    feedback: Option<PathBuf>,
    log: Option<(PathBuf, Redirect)>,
    err: &mut dyn Write,
) -> Result<(), Failure> {
    let internal = |e: io::Error| (exit::ERROR, e.to_string(), None);
    let code = |code: &str| if code == "conflict" { exit::CONFLICT } else { exit::ERROR };
    let server = Server::bind(socket).map_err(|e| (code(e.code()), e.to_string(), e.hint()))?;
    // Opened after the socket is ours, so a second daemon on the same socket
    // never touches the record. Held until the daemon stops.
    let stored = |e: StoreError| (code(e.code()), e.to_string(), e.hint());
    // A relative --feedback is from where slingerd started.
    let feedback = feedback.map(|f| std::path::absolute(&f).unwrap_or(f));
    let store = Store::open(dir, machine_name())
        .map_err(stored)?
        .with_claude(claude)
        .with_slinger(slinger)
        .with_feedback(feedback.unwrap_or_else(|| feedback::default_file(dir)));
    let log = log.map(|(path, redirect)| logging(&path, redirect, socket, dir)).transpose()?;
    warn_broken(store.workspaces(), err).map_err(internal)?;
    // The record's lock is ours: so is the name of its socket (docs/service.md).
    control::name_socket(dir, socket).map_err(internal)?;
    let stopper = server.stopper();
    // SIGHUP is caught and ignored: a closed terminal does not end the daemon.
    let mut signals = Signals::new([SIGTERM, SIGINT, SIGHUP]).map_err(internal)?;
    std::thread::spawn(move || {
        if signals.forever().any(|signal| signal != SIGHUP) {
            stopper.stop();
        }
    });
    let result = server.serve(&store).map_err(internal);
    control::unname_socket(dir).map_err(internal)?;
    if let (Some(file), Ok(())) = (&log, &result) {
        log::say(file, "stopped").map_err(internal)?;
    }
    result
}

/// The `--log` file at `path`, open, given to `redirect`, with the line that
/// says this daemon serves `dir` on `socket`.
fn logging(path: &Path, redirect: Redirect, socket: &Path, dir: &Path) -> Result<File, Failure> {
    let failed = |e: io::Error| {
        let hint = "pass a --log <PATH> that slingerd can write".to_string();
        (exit::ERROR, format!("cannot use log file {}: {e}", path.display()), Some(hint))
    };
    let file = log::open(path).map_err(failed)?;
    redirect(&file).map_err(failed)?;
    let serving = format!("serving {} on {}", dir.display(), socket.display());
    log::say(&file, &serving).map_err(failed)?;
    Ok(file)
}
