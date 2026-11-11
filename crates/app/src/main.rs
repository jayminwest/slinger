//! The `slinger` macOS app: a thin front end over `slingerd`. It does not
//! link the core.
//!
//! `backend` gets the data, `config` reads the config file, `commands` turns
//! keys into commands, `panes` keeps the window's panes and their focus,
//! `scene` decides what the window shows, `layout` does the geometry and
//! `platform` draws.
//! Only `platform` touches `AppKit`, and only this file starts it (gate
//! check:layers).

mod backend;
mod commands;
mod config;
mod layout;
mod panes;
mod platform;
mod scene;

use std::io::{self, Write};
use std::process::ExitCode;

use slinger_control as control;

use crate::backend::{Problem, Settings};
use crate::commands::Command;
use crate::layout::{Axis, Direction, Point};
use crate::panes::{Pane, Panes};
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
    // The workspace's own layout comes from the daemon from slinger-2241.
    let mut panes = Panes::new(Pane { kind: panes::Kind::Canvas, name: "main".to_string() });
    let first = view(&greeting, &settings, &panes, &tools);
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
                return Step::Draw(view(&greeting, &settings, &panes, &tools));
            }
            Event::Press(_) => return Step::Pass,
            Event::Key(chord) if tools.key(chord) => {
                return Step::Draw(view(&greeting, &settings, &panes, &tools));
            }
            Event::Key(chord) => {
                let kind = match settings.config.keys.command(chord) {
                    None => return Step::Pass,
                    Some(Command::Quit) => return Step::Quit,
                    Some(Command::ReloadConfig) => None,
                    Some(Command::PickColour) => Some(Kind::Colour),
                    Some(Command::PickShape) => Some(Kind::Shape),
                    Some(Command::PickTool) => Some(Kind::Tool),
                    Some(command) => {
                        pane_command(&mut panes, command);
                        return Step::Draw(view(&greeting, &settings, &panes, &tools));
                    }
                };
                if let Some(kind) = kind {
                    tools.open(kind, window);
                    return Step::Draw(view(&greeting, &settings, &panes, &tools));
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
        Step::Draw(view(&greeting, &settings, &panes, &tools))
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

/// Runs a pane command. Commands that are not about panes do nothing.
fn pane_command(panes: &mut Panes, command: Command) {
    let area = layout::WINDOW.rect();
    match command {
        Command::Quit
        | Command::ReloadConfig
        | Command::PickColour
        | Command::PickShape
        | Command::PickTool => {}
        Command::SplitRight => panes.split(Axis::Row),
        Command::SplitDown => panes.split(Axis::Column),
        Command::ClosePane => panes.close(),
        Command::FocusNext => panes.focus_next(),
        Command::FocusLeft => panes.focus_toward(Direction::Left, area),
        Command::FocusRight => panes.focus_toward(Direction::Right, area),
        Command::FocusUp => panes.focus_toward(Direction::Up, area),
        Command::FocusDown => panes.focus_toward(Direction::Down, area),
        Command::GrowPane => panes.grow(1.0),
        Command::ShrinkPane => panes.grow(-1.0),
    }
}

/// The window: the panes, any problem with the daemon or the config, and
/// the pop-up at the pen on top.
fn view(
    greeting: &Result<String, Problem>,
    settings: &Settings,
    panes: &Panes,
    tools: &Tools,
) -> Scene {
    let theme = &settings.config.theme;
    let problems: Vec<_> = greeting.as_ref().err().into_iter().chain(&settings.problem).collect();
    let mut scene = scene::build(panes, theme, &problems);
    scene.labels.extend(tools.labels(theme));
    scene
}

/// The daemon's greeting over `socket`. Blocks on the daemon.
fn greet(socket: &std::path::Path) -> Result<String, Problem> {
    backend::greeting(&control::Client::new(socket), "world")
}
