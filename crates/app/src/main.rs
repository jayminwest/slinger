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

use crate::backend::{Problem, Settings};
use crate::commands::Command;
use crate::layout::Point;
use crate::platform::{Event, Step};
use crate::scene::Scene;
use crate::scene::tools::{Kind, Tools};

fn main() -> ExitCode {
    let mut settings = backend::Settings::load(backend::ConfigFiles::locate());
    let mut socket = settings.socket();
    let mut greeting = greet(&socket);
    let window = layout::WINDOW;
    let centre = Point { x: window.width / 2.0, y: window.height / 2.0 };
    let mut tools = Tools::new(settings.config.palette.clone(), centre);
    let first = view(&greeting, &settings, &tools);
    // Runs on the main thread for each key press, pen or mouse event, and
    // tick of the window's timer. An open pop-up takes its keys before the
    // keymap. A tick stats the two config files; only an edit that moves the
    // socket asks the daemon again.
    let answer = move |event| {
        let force = match event {
            Event::Tick => false,
            Event::Moved(at) => {
                tools.moved(at);
                return Step::Stay;
            }
            Event::Press(at) if tools.press(at) => {
                return Step::Draw(view(&greeting, &settings, &tools));
            }
            Event::Press(_) => return Step::Pass,
            Event::Key(chord) if tools.key(chord) => {
                return Step::Draw(view(&greeting, &settings, &tools));
            }
            Event::Key(chord) => {
                let kind = match settings.config.keys.command(chord) {
                    None => return Step::Pass,
                    Some(Command::Quit) => return Step::Quit,
                    Some(Command::ReloadConfig) => None,
                    Some(Command::PickColour) => Some(Kind::Colour),
                    Some(Command::PickShape) => Some(Kind::Shape),
                    Some(Command::PickTool) => Some(Kind::Tool),
                };
                if let Some(kind) = kind {
                    tools.open(kind, window);
                    return Step::Draw(view(&greeting, &settings, &tools));
                }
                true
            }
        };
        if !settings.refresh(force) {
            return Step::Stay;
        }
        tools.set_palette(settings.config.palette.clone());
        let now = settings.socket();
        if now != socket {
            greeting = greet(&now);
            socket = now;
        }
        Step::Draw(view(&greeting, &settings, &tools))
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

/// The window: the greeting or its problem, any config problem, and the
/// pop-up at the pen on top.
fn view(greeting: &Result<String, Problem>, settings: &Settings, tools: &Tools) -> Scene {
    let theme = &settings.config.theme;
    let mut scene = scene::build(greeting, theme, settings.problem.as_ref());
    scene.labels.extend(tools.labels(theme));
    scene
}

/// The daemon's greeting over `socket`. Blocks on the daemon.
fn greet(socket: &std::path::Path) -> Result<String, Problem> {
    backend::greeting(&control::Client::new(socket), "world")
}
