//! `slinger drive`: send input to a driven app (`slinger-app --drive
//! PATH`, slinger-338dfc) and read back what its window shows.
//! Argv in, one request line to the app's socket out. This is the one
//! command that talks to the app, not to slingerd.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use clap::{Args, Subcommand, ValueEnum};
use serde_json::{Value, json};

use slinger_control::RemoteError;

use crate::env::Env;
use crate::output::Output;

/// Where the CLI finds the driven app's socket when `--app` is not given.
pub const DRIVE_ENV: &str = "SLINGER_DRIVE";

#[derive(Args)]
pub struct Drive {
    #[arg(long, value_name = "PATH", help = "The driven app's socket [default: $SLINGER_DRIVE]")]
    app: Option<PathBuf>,
    #[command(subcommand)]
    op: Op,
}

/// What made a pen event; a mouse with none of them.
#[derive(Args)]
#[expect(clippy::struct_excessive_bools, reason = "one flag for each fact of a pen event")]
struct Pen {
    /// A tablet pen made it: with the pen tool, only a tablet press draws
    #[arg(long)]
    tablet: bool,
    /// Shift was held
    #[arg(long)]
    shift: bool,
    /// A finger made it: a trackpad's press, or a palm
    #[arg(long)]
    touch: bool,
    /// The pen's eraser end made it
    #[arg(long)]
    eraser: bool,
}

/// Which mouse button, and whether control was held; the left with none.
#[derive(Args)]
struct Mouse {
    /// The button: right (a context click, or a drag that pans) or middle (a drag that pans)
    #[arg(long, value_enum, default_value_t = Which::Left)]
    button: Which,
    /// Control was held: a left press is the right button's, as macOS takes a ctrl+click
    #[arg(long)]
    control: bool,
}

/// A mouse button.
#[derive(Clone, Copy, ValueEnum)]
enum Which {
    Left,
    Right,
    Middle,
}

/// The help of a point: either form, as `drive scene` prints it.
const AT: &str = "The point, from the window's top left, as X,Y or X Y";

#[derive(Subcommand)]
enum Op {
    /// Press a key, such as cmd+j, escape or shift+a; prints taken or passed
    Key {
        /// The key and its modifiers: cmd+j, shift+a, escape, return, delete, left
        chord: String,
        /// The text it types when the app passes the key on [default: the key's character, none with cmd or ctrl]
        #[arg(long)]
        text: Option<String>,
    },
    /// Type text, a key press per character
    Type {
        /// The text; the words are joined with a space
        #[arg(required = true, num_args = 1..)]
        text: Vec<String>,
    },
    /// Press the pen or a mouse button at a point of the window, from its top left
    Press {
        #[arg(help = AT, value_name = "X,Y", num_args = 1..=2, required = true, allow_negative_numbers = true)]
        at: Vec<String>,
        #[command(flatten)]
        pen: Pen,
        #[command(flatten)]
        mouse: Mouse,
    },
    /// Move the pen or the mouse to a point while it is down
    Drag {
        #[arg(help = AT, value_name = "X,Y", num_args = 1..=2, required = true, allow_negative_numbers = true)]
        at: Vec<String>,
        #[command(flatten)]
        pen: Pen,
        #[command(flatten)]
        mouse: Mouse,
    },
    /// Lift the pen, or let a mouse button up, at a point
    Lift {
        #[arg(help = AT, value_name = "X,Y", num_args = 1..=2, required = true, allow_negative_numbers = true)]
        at: Vec<String>,
        #[command(flatten)]
        pen: Pen,
        #[command(flatten)]
        mouse: Mouse,
    },
    /// Press at the first point, drag through the others and lift at the last
    Stroke {
        /// The points, each as X,Y (or all as X Y X Y ...)
        #[arg(required = true, num_args = 1.., value_name = "X,Y", allow_negative_numbers = true)]
        points: Vec<String>,
        #[command(flatten)]
        pen: Pen,
    },
    /// Move the pointer to a point with no button down
    Move {
        #[arg(help = AT, value_name = "X,Y", num_args = 1..=2, required = true, allow_negative_numbers = true)]
        at: Vec<String>,
    },
    /// Scroll by DX,DY points over a point (X,Y DX,DY or X Y DX DY)
    #[command(override_usage = "slinger drive scroll [OPTIONS] X,Y DX,DY | X Y DX DY")]
    Scroll {
        /// The point, then how far: X,Y DX,DY or X Y DX DY; a negative DY scrolls up
        #[arg(value_name = "ARG", num_args = 2..=4, required = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Pinch over a point: zoom by a factor about it (X,Y FACTOR or X Y FACTOR)
    #[command(override_usage = "slinger drive zoom [OPTIONS] X,Y FACTOR | X Y FACTOR")]
    Zoom {
        /// The point, then the factor (over 1 zooms in): X,Y FACTOR or X Y FACTOR
        #[arg(value_name = "ARG", num_args = 2..=3, required = true)]
        args: Vec<String>,
    },
    /// Hold the space bar down, as for a pan with the pointer; --up lets it go
    Space {
        /// Let the space bar up
        #[arg(long)]
        up: bool,
    },
    /// Choose a command as its menu item does, by its name in the config file
    Run {
        /// The command's name, such as toggle-sidebar (the config file lists them)
        name: String,
    },
    /// Copy the selection to the driven window's own clipboard, never yours
    Copy,
    /// Cut the selection to the driven window's own clipboard
    Cut,
    /// Paste the text given, else what the driven window last cut or copied
    Paste {
        /// The text to paste; the words are joined with a space
        text: Vec<String>,
    },
    /// Print the text the driven window last cut or copied
    Clip,
    /// Drop a file on the window at a point (X,Y FILE or X Y FILE)
    #[command(override_usage = "slinger drive drop [OPTIONS] X,Y FILE | X Y FILE")]
    Drop {
        /// The point, then the file: X,Y FILE or X Y FILE
        #[arg(value_name = "ARG", num_args = 2..=3, required = true)]
        args: Vec<String>,
    },
    /// Let the window take the daemon's news now, as its timer does twice a second
    Tick,
    /// Print what the window shows, a line each: place, colour and text
    Scene {
        /// All of the scene, as Rust prints it
        #[arg(long)]
        debug: bool,
    },
    /// Write the window's content to a PNG file; prints its path
    Shot {
        /// The PNG file to write
        file: PathBuf,
    },
    /// Make the window's content W by H points
    Resize {
        /// The width, in points
        w: f64,
        /// The height, in points
        h: f64,
    },
}

impl Drive {
    pub fn name(&self) -> &'static str {
        match self.op {
            Op::Key { .. } => "drive key",
            Op::Type { .. } => "drive type",
            Op::Press { .. } => "drive press",
            Op::Drag { .. } => "drive drag",
            Op::Lift { .. } => "drive lift",
            Op::Stroke { .. } => "drive stroke",
            Op::Move { .. } => "drive move",
            Op::Scroll { .. } => "drive scroll",
            Op::Zoom { .. } => "drive zoom",
            Op::Space { .. } => "drive space",
            Op::Run { .. } => "drive run",
            Op::Copy => "drive copy",
            Op::Cut => "drive cut",
            Op::Paste { .. } => "drive paste",
            Op::Clip => "drive clip",
            Op::Drop { .. } => "drive drop",
            Op::Tick => "drive tick",
            Op::Scene { .. } => "drive scene",
            Op::Shot { .. } => "drive shot",
            Op::Resize { .. } => "drive resize",
        }
    }

    pub fn execute(&self, env: &Env) -> anyhow::Result<Output> {
        let Some(app) = self.app.as_deref().or(env.drive.as_deref()) else {
            let hint = format!("pass --app <PATH>, or set {DRIVE_ENV}");
            return Err(RemoteError::new("usage", "no driven app's socket", Some(hint)).into());
        };
        let reply = ask(app, &self.op.request()?)?;
        if reply["ok"] == true {
            let said = reply["said"].as_str().unwrap_or_default().trim_end().to_string();
            return Ok(Output::new(json!({ "said": said }), said));
        }
        let why = reply["error"].as_str().unwrap_or("the app gave no reply").to_string();
        Err(RemoteError::new("invalid", why, None).into())
    }
}

impl Op {
    /// The request line's object (crates/app/src/drive.rs reads it).
    fn request(&self) -> anyhow::Result<Value> {
        let penned = |op: &str, at: &[String], pen: &Pen, mouse: &Mouse| -> anyhow::Result<Value> {
            let [x, y] = numbers(at)?;
            let mut request = with(json!({ "op": op, "x": x, "y": y }), pen);
            let button = match mouse.button {
                Which::Left => "left",
                Which::Right => "right",
                Which::Middle => "middle",
            };
            (request["button"], request["control"]) = (json!(button), json!(mouse.control));
            Ok(request)
        };
        Ok(match self {
            Self::Key { chord, text } => json!({ "op": "key", "chord": chord, "text": text }),
            Self::Type { text } => json!({ "op": "type", "text": text.join(" ") }),
            Self::Press { at, pen, mouse } => penned("press", at, pen, mouse)?,
            Self::Drag { at, pen, mouse } => penned("drag", at, pen, mouse)?,
            Self::Lift { at, pen, mouse } => penned("lift", at, pen, mouse)?,
            Self::Stroke { points, pen } => {
                let flat =
                    split(points).filter(|n| n.len() % 2 == 0).ok_or_else(|| wrong(points))?;
                let points = flat.as_chunks::<2>().0;
                with(json!({ "op": "stroke", "points": points }), pen)
            }
            Self::Move { at } => {
                let [x, y] = numbers(at)?;
                json!({ "op": "move", "x": x, "y": y })
            }
            Self::Scroll { args } => {
                let [x, y, dx, dy] = numbers(args)?;
                json!({ "op": "scroll", "x": x, "y": y, "dx": dx, "dy": dy })
            }
            Self::Zoom { args } => {
                let [x, y, factor] = numbers(args)?;
                json!({ "op": "zoom", "x": x, "y": y, "factor": factor })
            }
            Self::Space { up } => json!({ "op": "space", "down": !up }),
            Self::Run { name } => json!({ "op": "command", "name": name }),
            Self::Copy => json!({ "op": "copy" }),
            Self::Cut => json!({ "op": "cut" }),
            Self::Paste { text } if text.is_empty() => json!({ "op": "paste" }),
            Self::Paste { text } => json!({ "op": "paste", "text": text.join(" ") }),
            Self::Clip => json!({ "op": "clip" }),
            Self::Drop { args } => {
                let mut args = args.clone();
                let file = args.pop().map(PathBuf::from).unwrap_or_default();
                let [x, y] = numbers(&args)?;
                json!({ "op": "drop", "x": x, "y": y, "path": std::path::absolute(file)? })
            }
            Self::Tick => json!({ "op": "tick" }),
            Self::Scene { debug } => json!({ "op": "scene", "debug": debug }),
            Self::Shot { file } => json!({ "op": "shot", "path": std::path::absolute(file)? }),
            Self::Resize { w, h } => json!({ "op": "resize", "w": w, "h": h }),
        })
    }
}

/// `request` with what made its pen event.
fn with(mut request: Value, pen: &Pen) -> Value {
    for (key, on) in
        [("tablet", pen.tablet), ("shift", pen.shift), ("touch", pen.touch), ("eraser", pen.eraser)]
    {
        request[key] = json!(on);
    }
    request
}

/// The `N` numbers `args` give, each arg one number or several joined
/// with commas: a point is `10,20` or `10 20`.
fn numbers<const N: usize>(args: &[String]) -> anyhow::Result<[f64; N]> {
    split(args).and_then(|n| n.try_into().ok()).ok_or_else(|| wrong(args))
}

/// The numbers in `args`, split at commas; none if one is not a number.
fn split(args: &[String]) -> Option<Vec<f64>> {
    args.iter().flat_map(|arg| arg.split(',')).map(|n| n.trim().parse().ok()).collect()
}

/// Why `args` are not the numbers an op takes.
fn wrong(args: &[String]) -> anyhow::Error {
    let hint = "write a point as X,Y or X Y (100,200 or 100 200); \
                `slinger drive <OP> --help` says what an op takes";
    let said = format!("not the numbers it takes: {}", args.join(" "));
    RemoteError::new("usage", said, Some(hint.to_string())).into()
}

/// Sends `request` to the app on `socket` and waits for its reply.
fn ask(socket: &Path, request: &Value) -> anyhow::Result<Value> {
    let gone = |e: std::io::Error| {
        let hint = "start one: slinger-app --drive <PATH>, with a config that names its daemon";
        RemoteError::new(
            "unavailable",
            format!("no driven app on {}: {e}", socket.display()),
            Some(hint.to_string()),
        )
    };
    let mut stream = slinger_control::connect_socket(socket).map_err(gone)?;
    writeln!(stream, "{request}").map_err(gone)?;
    let mut line = String::new();
    BufReader::new(&stream).read_line(&mut line).map_err(gone)?;
    Ok(serde_json::from_str(&line).unwrap_or(Value::Null))
}
