//! `slinger drive`: send input to a driven app (`slinger-app --drive
//! PATH`, slinger-338dfc) and read back what its window shows.
//! Argv in, one request line to the app's socket out. This is the one
//! command that talks to the app, not to slingerd.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};

use clap::{Args, Subcommand};
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

#[derive(Subcommand)]
enum Op {
    /// Press a key, such as cmd+j, escape or shift+a; prints taken or passed
    Key {
        chord: String,
        /// The text it types when the app passes the key on [default: the key's character, none with cmd or ctrl]
        #[arg(long)]
        text: Option<String>,
    },
    /// Type text, a key press per character
    Type {
        #[arg(required = true, num_args = 1..)]
        text: Vec<String>,
    },
    /// Press the pen or the mouse button at a point of the window, from its top left
    Press {
        x: f64,
        y: f64,
        #[command(flatten)]
        pen: Pen,
    },
    /// Move the pen or the mouse to a point while it is down
    Drag {
        x: f64,
        y: f64,
        #[command(flatten)]
        pen: Pen,
    },
    /// Lift the pen, or let the mouse button up, at a point
    Lift {
        x: f64,
        y: f64,
        #[command(flatten)]
        pen: Pen,
    },
    /// Press at the first point, drag through the others and lift at the last
    Stroke {
        /// The points, each as x,y
        #[arg(required = true, num_args = 1.., value_name = "X,Y")]
        points: Vec<String>,
        #[command(flatten)]
        pen: Pen,
    },
    /// Move the pointer to a point with no button down
    Move { x: f64, y: f64 },
    /// Scroll by dx,dy points over a point
    Scroll {
        x: f64,
        y: f64,
        #[arg(allow_negative_numbers = true)]
        dx: f64,
        #[arg(allow_negative_numbers = true)]
        dy: f64,
    },
    /// Pinch over a point: zoom by a factor about it
    Zoom { x: f64, y: f64, factor: f64 },
    /// Choose a command as its menu item does, by its name in the config file
    Run { name: String },
    /// Copy the selection to the driven window's own clipboard, never yours
    Copy,
    /// Cut the selection to the driven window's own clipboard
    Cut,
    /// Paste the text given, else what the driven window last cut or copied
    Paste { text: Vec<String> },
    /// Print the text the driven window last cut or copied
    Clip,
    /// Drop a file on the window at a point
    Drop { x: f64, y: f64, file: PathBuf },
    /// Let the window take the daemon's news now, as its timer does twice a second
    Tick,
    /// Print what the window shows, a line each: place, colour and text
    Scene {
        /// All of the scene, as Rust prints it
        #[arg(long)]
        debug: bool,
    },
    /// Write the window's content to a PNG file; prints its path
    Shot { file: PathBuf },
    /// Make the window's content w by h points
    Resize { w: f64, h: f64 },
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
        let penned =
            |op: &str, x: f64, y: f64, pen: &Pen| with(json!({ "op": op, "x": x, "y": y }), pen);
        Ok(match self {
            Self::Key { chord, text } => json!({ "op": "key", "chord": chord, "text": text }),
            Self::Type { text } => json!({ "op": "type", "text": text.join(" ") }),
            Self::Press { x, y, pen } => penned("press", *x, *y, pen),
            Self::Drag { x, y, pen } => penned("drag", *x, *y, pen),
            Self::Lift { x, y, pen } => penned("lift", *x, *y, pen),
            Self::Stroke { points, pen } => {
                let points: Option<Vec<[f64; 2]>> = points.iter().map(|p| point(p)).collect();
                let Some(points) = points else {
                    let hint = "write each point as x,y: 100,200 140.5,220".to_string();
                    return Err(RemoteError::new("usage", "a point is not x,y", Some(hint)).into());
                };
                with(json!({ "op": "stroke", "points": points }), pen)
            }
            Self::Move { x, y } => json!({ "op": "move", "x": x, "y": y }),
            Self::Scroll { x, y, dx, dy } => {
                json!({ "op": "scroll", "x": x, "y": y, "dx": dx, "dy": dy })
            }
            Self::Zoom { x, y, factor } => {
                json!({ "op": "zoom", "x": x, "y": y, "factor": factor })
            }
            Self::Run { name } => json!({ "op": "command", "name": name }),
            Self::Copy => json!({ "op": "copy" }),
            Self::Cut => json!({ "op": "cut" }),
            Self::Paste { text } if text.is_empty() => json!({ "op": "paste" }),
            Self::Paste { text } => json!({ "op": "paste", "text": text.join(" ") }),
            Self::Clip => json!({ "op": "clip" }),
            Self::Drop { x, y, file } => {
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

/// `x,y` as a point.
fn point(text: &str) -> Option<[f64; 2]> {
    let (x, y) = text.split_once(',')?;
    Some([x.trim().parse().ok()?, y.trim().parse().ok()?])
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
    let mut stream = UnixStream::connect(socket).map_err(gone)?;
    writeln!(stream, "{request}").map_err(gone)?;
    let mut line = String::new();
    BufReader::new(&stream).read_line(&mut line).map_err(gone)?;
    Ok(serde_json::from_str(&line).unwrap_or(Value::Null))
}
