//! The `slinger` macOS app: a thin front end over `slingerd`. It does not
//! link the core.
//!
//! `backend` gets the data, `config` reads the config file, `commands` turns
//! keys into commands, `scene` decides what the window shows, `layout` does
//! the geometry and `platform` draws.
//! Only `platform` touches `AppKit`, and only this file starts it (gate
//! check:layers).

mod backend;
mod commands;
mod config;
mod layout;
mod platform;
mod scene;

use std::io::{self, Write};
use std::process::ExitCode;

use slinger_control as control;

use crate::commands::Command;
use crate::platform::{Event, Step};

fn main() -> ExitCode {
    let mut settings = backend::Settings::load(backend::ConfigFiles::locate());
    let mut socket = settings.socket();
    let mut greeting = greet(&socket);
    let first = scene::build(&greeting, &settings.config.theme, settings.problem.as_ref());
    // Runs on the main thread for each key press and each tick of the
    // window's timer. A tick stats the two config files; only an edit that
    // moves the socket asks the daemon again.
    let answer = move |event| {
        let force = match event {
            Event::Tick => false,
            Event::Key(chord) => match settings.config.keys.command(chord) {
                None => return Step::Pass,
                Some(Command::Quit) => return Step::Quit,
                Some(Command::ReloadConfig) => true,
            },
        };
        if !settings.refresh(force) {
            return Step::Stay;
        }
        let now = settings.socket();
        if now != socket {
            greeting = greet(&now);
            socket = now;
        }
        Step::Draw(scene::build(&greeting, &settings.config.theme, settings.problem.as_ref()))
    };
    match platform::run(&first, answer) {
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
