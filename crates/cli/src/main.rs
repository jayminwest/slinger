//! The `slinger` CLI: a thin front end over `slingerd`. It turns argv into
//! control-socket requests (docs/service.md) and replies into the output
//! contract in docs/cli.md. It does not link the core.
mod cli;
mod output;
#[cfg(test)]
mod tests;

use std::io::{self, IsTerminal};
use std::process::ExitCode;

fn main() -> ExitCode {
    let color = output::color_enabled(
        std::env::var_os("NO_COLOR").is_some(),
        std::env::var("FORCE_COLOR").ok().as_deref(),
        io::stdout().is_terminal(),
    );
    let code =
        cli::run(std::env::args_os(), &mut io::stdout().lock(), &mut io::stderr().lock(), color);
    ExitCode::from(code)
}
