//! The `slinger` macOS app: a thin front end over `slingerd`. It does not
//! link the core.
//!
//! `backend` gets the data, `scene` decides what the window shows, `layout`
//! does the geometry and `platform` draws. Only `platform` touches `AppKit`, and
//! only this file starts it (gate check:layers).

mod backend;
mod layout;
mod platform;
mod scene;

use std::io::{self, Write};
use std::process::ExitCode;

use slinger_control as control;
fn main() -> ExitCode {
    let daemon = control::Client::new(control::default_socket());
    let scene = scene::build(backend::greeting(&daemon, "world"));
    match platform::run(&scene) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            // Nothing can be done if stderr is closed too.
            let _ = writeln!(io::stderr(), "✗ {e}");
            ExitCode::FAILURE
        }
    }
}
