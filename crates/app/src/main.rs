//! The `slinger` macOS app: a thin front end over `slingerd`. It does not
//! link the core.
//!
//! `backend` gets the data, `config` reads the config file, `scene` decides
//! what the window shows, `layout` does the geometry and `platform` draws.
//! Only `platform` touches `AppKit`, and only this file starts it (gate
//! check:layers).

mod backend;
mod config;
mod layout;
mod platform;
mod scene;

use std::io::{self, Write};
use std::process::ExitCode;

use slinger_control as control;

fn main() -> ExitCode {
    let mut settings = backend::Settings::load(backend::ConfigFiles::locate());
    let mut socket = settings.socket();
    let mut greeting = greet(&socket);
    let first = scene::build(&greeting, &settings.config.theme, settings.problem.as_ref());
    // Runs on the main thread from the window's timer. It stats the two
    // config files; only an edit that moves the socket asks the daemon again.
    let next = move || {
        if !settings.refresh() {
            return None;
        }
        let now = settings.socket();
        if now != socket {
            greeting = greet(&now);
            socket = now;
        }
        Some(scene::build(&greeting, &settings.config.theme, settings.problem.as_ref()))
    };
    match platform::run(&first, next) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            // Nothing can be done if stderr is closed too.
            let _ = writeln!(io::stderr(), "✗ {e}");
            ExitCode::FAILURE
        }
    }
}

/// The daemon's greeting over `socket`. Blocks on the daemon.
fn greet(socket: &std::path::Path) -> Result<String, backend::Problem> {
    backend::greeting(&control::Client::new(socket), "world")
}
