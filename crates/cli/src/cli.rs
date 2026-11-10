//! Argv in, exit code out. All IO goes through the streams passed to [`run`], so
//! tests drive the whole CLI in-process. Add a command: a `Command` variant (its
//! doc comment is its help), a `name` arm and an `execute` arm.

use std::ffi::OsString;
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::PathBuf;

use clap::builder::styling::{AnsiColor, Styles};
use clap::error::ErrorKind;
use clap::{Args, CommandFactory, Parser, Subcommand};
use serde_json::{Value, json};

use slinger_control as control;

use crate::canvas::Canvas;
use crate::output::{Ctx, Failure, Output, exit};
use crate::session::{Session, Status};
use crate::ui::Ui;
use crate::workspace::Workspace;

/// The binary's name, as the user types it.
pub const NAME: &str = "slinger";

/// Help for `--socket`: a string, because a doc comment may not hold `$XDG_RUNTIME_DIR` bare.
const SOCKET_HELP: &str =
    "Daemon control socket [default: in $XDG_RUNTIME_DIR, else in the temporary directory]";

/// Help for an event's kind: a string, because a doc comment wants the kind names in backticks.
const KIND_HELP: &str = "The event's kind: sling_sent, reply_landed, file_touched, pane_moved, \
     agent_blocked or agent_done";

const STYLES: Styles = Styles::styled()
    .header(AnsiColor::White.on_default().bold())
    .usage(AnsiColor::White.on_default().bold())
    .literal(AnsiColor::Green.on_default())
    .placeholder(AnsiColor::BrightBlack.on_default());

#[derive(Parser)]
#[command(
    name = "slinger",
    about = "One window onto your computers: think with a pen on a canvas, sling anything to an agent, and get things back instead of chat.",
    version,
    disable_version_flag = true,
    disable_help_subcommand = true,
    styles = STYLES,
    help_template = "{name} v{version} — {about}\n\n{usage-heading} {usage}\n\n{all-args}{after-help}",
    after_help = "Run 'slinger <command> --help' for command-specific help."
)]
#[expect(clippy::struct_excessive_bools, reason = "independent global flags")]
struct Cli {
    /// Machine-readable JSON output
    #[arg(long, global = true)]
    json: bool,
    /// Silence progress and confirmations
    #[arg(short, long, global = true)]
    quiet: bool,
    /// Extra diagnostics on stderr
    #[arg(long, global = true)]
    verbose: bool,
    /// Print the version
    #[arg(short = 'v', long)]
    version: bool,
    #[arg(long, global = true, value_name = "PATH", help = SOCKET_HELP)]
    socket: Option<PathBuf>,
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Print a greeting
    Hello {
        /// Who to greet
        #[arg(default_value = "world")]
        name: String,
    },
    /// Read and write this machine's record of events
    #[command(subcommand)]
    Record(Record),
    /// Store and fetch blobs: bytes named by their hash
    #[command(subcommand)]
    Blob(Blob),
    /// Store, list and export agent UIs: HTML, named and versioned
    #[command(subcommand)]
    Ui(Ui),
    /// Make, read and change canvases: pages of strokes, text, boxes and arrows
    #[command(subcommand)]
    Canvas(Canvas),
    /// Open, list, show and close workspaces: a name, directories, an agent and a canvas
    #[command(subcommand)]
    Workspace(Workspace),
    /// List the agents slingerd can reach, and the status each reports
    #[command(subcommand)]
    Session(Session),
    /// Report the calling agent's status: working, blocked or done
    #[command(subcommand)]
    Status(Status),
}

#[derive(Subcommand)]
enum Blob {
    /// Store a file's bytes; prints the blob id
    Put {
        /// The file, or - to read standard input
        file: PathBuf,
    },
    /// Write a blob's bytes to standard output, or to a file
    Get {
        /// The blob id, sha256:<hex>
        id: String,
        /// Write to this file instead, replacing it; prints nothing
        #[arg(long, value_name = "FILE")]
        out: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
enum Record {
    /// Append an event; prints its number
    Append {
        #[arg(help = KIND_HELP)]
        kind: String,
        /// The payload, a JSON object that fits the kind
        #[arg(default_value = "{}")]
        payload: String,
        /// The program or agent that writes the event
        #[arg(long, value_name = "NAME", default_value = NAME)]
        source: String,
        #[command(flatten)]
        tags: TagArgs,
    },
    /// List events, lowest number first: number, kind, tags and payload per line
    List {
        /// Only events of this kind
        #[arg(long)]
        kind: Option<String>,
        /// Only events from this machine
        #[arg(long, value_name = "NAME")]
        machine: Option<String>,
        #[command(flatten)]
        tags: TagArgs,
        /// Only events numbered above this
        #[arg(long, value_name = "NUMBER")]
        after: Option<u64>,
    },
}

/// The tags a writer sets, and a reader filters by.
#[derive(Args)]
struct TagArgs {
    /// The workspace tag
    #[arg(long, value_name = "NAME")]
    workspace: Option<String>,
    /// The agent tag
    #[arg(long, value_name = "NAME")]
    agent: Option<String>,
    /// The canvas tag
    #[arg(long, value_name = "ID")]
    canvas: Option<String>,
}

impl Command {
    fn name(&self) -> &'static str {
        match self {
            Self::Hello { .. } => "hello",
            Self::Record(Record::Append { .. }) => "record append",
            Self::Record(Record::List { .. }) => "record list",
            Self::Blob(Blob::Put { .. }) => "blob put",
            Self::Blob(Blob::Get { .. }) => "blob get",
            Self::Ui(ui) => ui.name(),
            Self::Canvas(canvas) => canvas.name(),
            Self::Workspace(workspace) => workspace.name(),
            Self::Session(session) => session.name(),
            Self::Status(status) => status.name(),
        }
    }
}

fn execute(
    command: &Command,
    daemon: &control::Client,
    input: &mut dyn Read,
    ctx: &mut Ctx,
) -> anyhow::Result<Output> {
    match command {
        Command::Hello { name } => {
            ctx.debug(&format!("greeting {name:?}"))?;
            let greeting = daemon.hello(name)?;
            Ok(Output::new(json!({ "greeting": greeting }), greeting))
        }
        Command::Record(Record::Append { kind, payload, source, tags }) => {
            let payload = serde_json::from_str(payload).map_err(|e| {
                let hint = "pass a JSON object, such as '{\"summary\":\"done\"}'".to_string();
                control::RemoteError::new(
                    "invalid",
                    format!("payload is not JSON: {e}"),
                    Some(hint),
                )
            })?;
            let tags = control::NewTags {
                workspace: tags.workspace.clone(),
                agent: tags.agent.clone(),
                canvas: tags.canvas.clone(),
            };
            let new =
                control::NewEvent { kind: kind.clone(), source: source.clone(), tags, payload };
            let event = daemon.record_append(new)?;
            let number = event["number"].to_string();
            Ok(Output::new(json!({ "event": event }), number))
        }
        Command::Record(Record::List { kind, machine, tags, after }) => {
            let filter = control::EventFilter {
                kind: kind.clone(),
                machine: machine.clone(),
                workspace: tags.workspace.clone(),
                agent: tags.agent.clone(),
                canvas: tags.canvas.clone(),
                after: *after,
            };
            let events = daemon.record_list(filter)?;
            let human = events.iter().map(event_line).collect::<Vec<_>>().join("\n");
            Ok(Output::new(json!({ "events": events }), human))
        }
        Command::Blob(Blob::Put { file }) => {
            let id = if file.as_os_str() == "-" {
                let (scratch, mut copy) = Scratch::new()?;
                io::copy(input, &mut copy)?;
                daemon.blob_put(&scratch.0)?
            } else {
                daemon.blob_put(file)?
            };
            Ok(Output::new(json!({ "id": id }), id))
        }
        Command::Blob(Blob::Get { id, out: Some(out) }) => {
            let size = daemon.blob_get(id, out)?;
            Ok(Output::new(json!({ "id": id, "size": size }), String::new()))
        }
        Command::Blob(Blob::Get { id, out: None }) => {
            if ctx.json {
                let hint = "pass --out <FILE>, or drop --json".to_string();
                let message = "--json cannot share standard output with a blob's bytes";
                return Err(control::RemoteError::new("usage", message, Some(hint)).into());
            }
            let (scratch, _) = Scratch::new()?;
            daemon.blob_get(id, &scratch.0)?;
            io::copy(&mut File::open(&scratch.0)?, ctx.out)?;
            Ok(Output::new(json!({}), String::new()))
        }
        Command::Ui(ui) => ui.execute(daemon, input),
        Command::Canvas(canvas) => crate::canvas::execute(canvas, daemon, input),
        Command::Workspace(workspace) => crate::workspace::execute(workspace, daemon),
        Command::Session(session) => crate::session::execute(session, daemon),
        Command::Status(status) => crate::session::set(status, daemon),
    }
}

/// A file of this user's in the temporary directory that the daemon reads or
/// writes in the CLI's place: bytes travel as files, not in the request line
/// (docs/service.md). Removed on drop.
pub struct Scratch(pub PathBuf);

impl Scratch {
    pub fn new() -> io::Result<(Self, File)> {
        static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.subsec_nanos());
        let name = format!("{NAME}-{}-{n}-{nanos}.blob", std::process::id());
        let path = std::env::temp_dir().join(name);
        let file = File::options().write(true).create_new(true).mode(0o600).open(&path)?;
        Ok((Self(path), file))
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        // Nothing to do if it is already gone; the temporary directory is swept anyway.
        let _ = fs::remove_file(&self.0);
    }
}

/// One event for people: number, kind, tags and payload, separated by tabs.
fn event_line(event: &Value) -> String {
    let tags: Vec<String> = ["machine", "workspace", "agent", "canvas"]
        .into_iter()
        .filter_map(|tag| event["tags"][tag].as_str().map(|value| format!("{tag}={value}")))
        .collect();
    let kind = event["kind"].as_str().unwrap_or_default();
    format!("{}\t{kind}\t{}\t{}", event["number"], tags.join(" "), event["payload"])
}

pub fn run<I, T>(
    args: I,
    input: &mut dyn Read,
    out: &mut dyn Write,
    err: &mut dyn Write,
    color: bool,
) -> u8
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    let args: Vec<OsString> = args.into_iter().map(Into::into).collect();
    let json = args.iter().any(|a| a == "--json");
    let result = match Cli::try_parse_from(&args) {
        Ok(cli) => dispatch(&cli, input, out, err, color),
        Err(e) => parse_error(&e, Ctx::new(out, err, json, color)),
    };
    // A closed stdout (`| head`) is success, not a crash.
    result.unwrap_or_else(|e| if is_broken_pipe(&e) { exit::OK } else { exit::ERROR })
}

fn is_broken_pipe(e: &io::Error) -> bool {
    e.kind() == io::ErrorKind::BrokenPipe
}

fn dispatch(
    cli: &Cli,
    input: &mut dyn Read,
    out: &mut dyn Write,
    err: &mut dyn Write,
    color: bool,
) -> io::Result<u8> {
    let mut ctx = Ctx::new(out, err, cli.json, color);
    ctx.quiet = cli.quiet;
    ctx.verbose = cli.verbose;
    if cli.version {
        return version(&mut ctx);
    }
    let Some(command) = &cli.command else {
        let help = Cli::command().render_help();
        let text = if ctx.color { help.ansi().to_string() } else { help.to_string() };
        write!(ctx.out, "{text}")?;
        return Ok(exit::OK);
    };
    ctx.command = command.name();
    let socket = cli.socket.clone().unwrap_or_else(control::default_socket);
    let daemon = control::Client::new(socket);
    match execute(command, &daemon, input, &mut ctx) {
        Ok(output) => ctx.emit(&output),
        // A closed stdout is the reader's choice; `run` turns it into success.
        Err(e) if e.downcast_ref::<io::Error>().is_some_and(is_broken_pipe) => {
            Err(io::ErrorKind::BrokenPipe.into())
        }
        Err(e) => ctx.fail(&Failure::from(&e)),
    }
}

fn version(ctx: &mut Ctx) -> io::Result<u8> {
    let v = env!("CARGO_PKG_VERSION");
    if !ctx.json {
        writeln!(ctx.out, "{v}")?;
        return Ok(exit::OK);
    }
    ctx.command = "version";
    let platform = format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH);
    ctx.emit(&Output::new(
        json!({ "name": NAME, "version": v, "platform": platform }),
        String::new(),
    ))
}

fn parse_error(e: &clap::Error, mut ctx: Ctx) -> io::Result<u8> {
    if matches!(e.kind(), ErrorKind::DisplayHelp | ErrorKind::DisplayVersion) {
        let text = if ctx.color { e.render().ansi().to_string() } else { e.render().to_string() };
        write!(ctx.out, "{text}")?;
        return Ok(exit::OK);
    }
    let rendered = e.render().to_string();
    let mut lines = rendered.lines().map(str::trim).filter(|l| !l.is_empty());
    let message = lines.next().unwrap_or("invalid arguments").trim_start_matches("error: ");
    let tip = lines.find_map(|l| l.strip_prefix("tip: "));
    let hint = tip.map_or_else(|| format!("run '{NAME} --help'"), str::to_string);
    ctx.fail(&Failure::usage(message, hint))
}
