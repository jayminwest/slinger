//! Writes a synthetic canvas file of realistic shape, for measuring
//! (slinger-6c0f10; `docs/perf/canvas.md`). The same arguments give the same
//! bytes. Put it at `canvases/<ID>.canvas` in a scratch daemon's data
//! directory before the daemon starts:
//!
//! ```sh
//! cargo run --release -p slingerd --example synthetic_canvas -- data/canvases/big.canvas --id big
//! ```

use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;
use slinger_core::canvas::file::{SESSIONS, Synthetic};

#[derive(Parser)]
struct Args {
    /// The file to write. One that is there is replaced.
    out: PathBuf,
    /// The canvas id, as in the file's name.
    #[arg(long, default_value = "big")]
    id: String,
    /// How many times the default content (a little over 5 MB): more
    /// sittings on the same pages.
    #[arg(long, default_value_t = 1)]
    scale: u32,
    /// The sittings of writing, in place of --scale: 6 makes a small canvas.
    #[arg(long)]
    sessions: Option<u32>,
    /// Another seed, another canvas of the same shape.
    #[arg(long, default_value_t = 1)]
    seed: u64,
    /// The blob id of the PDF the canvas shows (`slinger blob put`).
    #[arg(long)]
    pdf: Option<String>,
}

#[expect(clippy::print_stderr, reason = "an example's only output is its error")]
fn main() -> ExitCode {
    let args = Args::parse();
    let sessions = args.sessions.unwrap_or(SESSIONS.saturating_mul(args.scale.max(1)));
    let mut make = Synthetic { sessions, seed: args.seed, ..Synthetic::default() };
    if let Some(pdf) = args.pdf {
        make.pdf = pdf;
    }
    let written = make.file(&args.id).map_err(|e| e.to_string()).and_then(|bytes| {
        std::fs::write(&args.out, bytes).map_err(|e| format!("{}: {e}", args.out.display()))
    });
    match written {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("✗ {e}");
            ExitCode::FAILURE
        }
    }
}
