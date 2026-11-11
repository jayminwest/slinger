//! The `slinger` CLI: a thin front end over `slingerd`. It turns argv into
//! control-socket requests (docs/service.md) and replies into the output
//! contract in docs/cli.md. It does not link the core.
mod canvas;
mod cli;
mod edit;
mod feedback;
mod look;
mod output;
mod pane;
mod put;
mod session;
mod sling;
#[cfg(test)]
mod tests;
mod ui;
mod workspace;

use std::io::{self, IsTerminal};
use std::process::ExitCode;

fn main() -> ExitCode {
    let color = output::color_enabled(
        std::env::var_os("NO_COLOR").is_some(),
        std::env::var("FORCE_COLOR").ok().as_deref(),
        io::stdout().is_terminal(),
    );
    let (mut input, mut out, mut err) =
        (io::stdin().lock(), io::stdout().lock(), io::stderr().lock());
    let code = cli::run(std::env::args_os(), &mut input, &mut out, &mut err, color);
    ExitCode::from(code)
}
