//! Argv in, exit code out. All IO goes through the writers passed to [`run`], so
//! tests drive the whole CLI in-process. Add a command: a `Command` variant (its
//! doc comment is its help), a `name` arm and an `execute` arm.

use std::ffi::OsString;
use std::io::{self, Write};
use std::path::PathBuf;

use clap::builder::styling::{AnsiColor, Styles};
use clap::error::ErrorKind;
use clap::{Args, CommandFactory, Parser, Subcommand};
use serde_json::{Value, json};

use slinger_control as control;

use crate::output::{Ctx, Failure, Output, exit};

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
        }
    }
}

fn execute(command: &Command, daemon: &control::Client, ctx: &mut Ctx) -> anyhow::Result<Output> {
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

pub fn run<I, T>(args: I, out: &mut dyn Write, err: &mut dyn Write, color: bool) -> u8
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    let args: Vec<OsString> = args.into_iter().map(Into::into).collect();
    let json = args.iter().any(|a| a == "--json");
    let result = match Cli::try_parse_from(&args) {
        Ok(cli) => dispatch(&cli, out, err, color),
        Err(e) => parse_error(&e, Ctx::new(out, err, json, color)),
    };
    // A closed stdout (`| head`) is success, not a crash.
    result.unwrap_or_else(|e| {
        if e.kind() == io::ErrorKind::BrokenPipe { exit::OK } else { exit::ERROR }
    })
}

fn dispatch(cli: &Cli, out: &mut dyn Write, err: &mut dyn Write, color: bool) -> io::Result<u8> {
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
    match execute(command, &daemon, &mut ctx) {
        Ok(output) => ctx.emit(&output),
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
